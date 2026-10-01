//! PDF export via WebView2's native `PrintToPdfAsync` (issue #256's final fix).
//!
//! Two earlier attempts both routed through `window.print()` — the browser-style native print
//! dialog — first from a hidden `<iframe>` inside the main window, then from a genuine separate
//! top-level `WebviewWindow`. Both rendered the print preview correctly in a packaged release
//! build, but clicking "印刷" never produced the OS "Save As" file picker and no PDF was ever
//! written, regardless of which window/document triggered `print()`. A control test (printing a
//! normal top-level page in a different Chromium-based browser) confirmed the OS "Microsoft Print
//! to PDF" pipeline itself works fine on the test machine — so the failure was specific to how
//! this app's packaged WebView2 hosting reaches the native Save-As step from `window.print()`.
//!
//! Microsoft's own WebView2 docs (<https://learn.microsoft.com/en-us/microsoft-edge/webview2/how-to/print>)
//! confirm `window.print()` maps to `ShowPrintUI()`'s **Browser** dialog kind — the same UI that
//! was failing. `ICoreWebView2.PrintToPdfAsync` is a separate, dedicated, **silent, no-dialog**
//! API for exactly this use case: it takes a destination file path directly and writes the PDF
//! there, with no OS print dialog or Save-As picker involved at all. This module calls that API
//! directly via raw WebView2 COM bindings (`webview2-com`) — the only way to reach it, since
//! neither Tauri's own `Webview::print()` (macOS-only per its own doc comment) nor `wry` expose
//! any higher-level wrapper for it. The frontend (`src/editor/pdfExport.ts`) instead shows this
//! module's own native save-file dialog (`tauri_plugin_dialog`, the same crate/pattern
//! `commands::tab::tab_create` already uses for folder-pick) to choose the destination path, then
//! calls [`pdf_export_write`] with the already-rendered, self-contained HTML (images already
//! inlined as `data:` URIs by the frontend) to write there.
//!
//! # Threading
//!
//! `WebviewWindowBuilder::build()`'s own doc comment warns it deadlocks if called synchronously
//! from the main/event-loop thread (it internally dispatches to that thread and waits) — so window
//! creation below happens on a plain background thread (`tauri::async_runtime::spawn_blocking`),
//! matching that doc's own "create a window in a separate thread" example. The WebView2 COM calls
//! themselves (`ICoreWebView2`/`ICoreWebView2Controller` etc.) are STA-affine to the thread that
//! owns them — Windows' own real UI/event-loop main thread — so those specifically run via
//! [`tauri::WebviewWindow::run_on_main_thread`] instead, each waited on via a one-shot `mpsc`
//! channel (not `.await`-able; `wait_with_pump` pumps the Win32 message loop while blocking,
//! required for WebView2's own COM callbacks to ever be delivered — see `webview2_com::wait_with_pump`'s
//! own doc comment).

use std::path::Path;
use std::sync::mpsc;

use tauri::{AppHandle, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_dialog::DialogExt;
use webview2_com::Microsoft::Web::WebView2::Win32::{ICoreWebView2, ICoreWebView2Controller, ICoreWebView2_7};
use webview2_com::{NavigationCompletedEventHandler, PrintToPdfCompletedHandler};
use windows::core::{Interface, BOOL, HRESULT, HSTRING};

use crate::model::result::{AppResult, ErrorCode};

/// Characters invalid in a Windows file name, plus ASCII control characters — replaced with `_`
/// when deriving the save dialog's suggested file name from a memo name (memo names are free text
/// and may contain any of these, e.g. `:`/`?` in a title).
const INVALID_FILE_NAME_CHARS: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// Shows a native save-file dialog (suggesting `{memoName}.pdf`) and, if the user picks a
/// destination, renders `html` to that path via WebView2's `PrintToPdfAsync` — no further dialog
/// of any kind after this point (see this module's header comment for why `window.print()`'s
/// native print dialog was abandoned). Returns `Ok(None)` (not an error) if the user cancels the
/// save dialog, mirroring `commands::tab::tab_create`'s own "cancel is not a failure" convention.
#[tauri::command]
pub async fn pdf_export_write(app: AppHandle, html: String, memo_name: String) -> AppResult<Option<String>> {
    // Non-blocking save dialog (`tab_create`'s own doc comment explains why: the blocking
    // variant stalls every other in-flight `invoke()` call for as long as the dialog stays open).
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .add_filter("PDF", &["pdf"])
        .set_file_name(suggested_file_name(&memo_name))
        .save_file(move |picked| {
            let _ = tx.send(picked);
        });
    let picked = match rx.await {
        Ok(picked) => picked,
        Err(_) => return AppResult::err_code(ErrorCode::Unknown, "save dialog task was dropped"),
    };
    let Some(file_path) = picked else {
        tracing::info!("pdf_export_write: save dialog cancelled");
        return AppResult::ok(None);
    };
    let dest_path = match file_path.into_path() {
        Ok(path) => path,
        Err(err) => return AppResult::err_code(ErrorCode::Unknown, format!("invalid save path: {err}")),
    };
    tracing::info!(dest_path = %dest_path.display(), "pdf_export_write: save dialog returned path");

    let app_for_render = app.clone();
    let dest_path_for_render = dest_path.clone();
    let render_result = tauri::async_runtime::spawn_blocking(move || render_pdf(&app_for_render, html, &dest_path_for_render)).await;

    match render_result {
        Ok(Ok(())) => {
            tracing::info!(dest_path = %dest_path.display(), "pdf_export_write: render succeeded");
            AppResult::ok(Some(dest_path.to_string_lossy().into_owned()))
        }
        Ok(Err(message)) => {
            tracing::error!(error = %message, "pdf_export_write: render failed");
            AppResult::err_code(ErrorCode::Unknown, message)
        }
        Err(join_err) => {
            tracing::error!(error = %join_err, "pdf_export_write: render task panicked");
            AppResult::err_code(ErrorCode::Unknown, "PDF render task panicked")
        }
    }
}

fn suggested_file_name(memo_name: &str) -> String {
    let sanitized: String = memo_name
        .chars()
        .map(|c| if INVALID_FILE_NAME_CHARS.contains(&c) || c.is_control() { '_' } else { c })
        .collect();
    // Trailing dots/spaces are also invalid in a Windows file name (silently stripped by the
    // shell otherwise, but stripping explicitly here keeps the suggested name predictable).
    let trimmed = sanitized.trim_end_matches(['.', ' ']);
    let base = if trimmed.is_empty() { "memo" } else { trimmed };
    format!("{base}.pdf")
}

/// Unique per-call window label — export can be triggered again before a previous export's window
/// has finished tearing down.
fn print_window_label() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("ttn-pdf-export-{nanos}")
}

/// Creates a hidden print window, navigates it to `html`, and writes the rendered PDF to
/// `dest_path`. Runs on whatever thread calls it (a `spawn_blocking` background thread — see this
/// module's header comment on why `.build()` must not run on the main thread).
fn render_pdf(app: &AppHandle, html: String, dest_path: &Path) -> Result<(), String> {
    let label = print_window_label();
    // Real on-screen dimensions (not zero — the root cause of this module's very first, since-
    // superseded fix attempt for issue #256: a zero-size surface gets treated as having nothing
    // printable/paginatable) but never shown (`visible(false)`) — `PrintToPdfAsync`, unlike
    // `window.print()`'s native print-preview UI, never needs to actually be seen on screen.
    let window = WebviewWindowBuilder::new(app, &label, WebviewUrl::External("about:blank".parse().expect("\"about:blank\" is always a valid URL")))
        .visible(false)
        .decorations(false)
        .skip_taskbar(true)
        .inner_size(820.0, 1160.0)
        .build()
        .map_err(|err| format!("failed to create print window: {err}"))?;
    tracing::info!(label = %label, "render_pdf: print window created");

    let result = run_print_to_pdf(&window, html, dest_path);
    tracing::info!(?result, "render_pdf: run_print_to_pdf finished, closing print window");
    let close_result = window.close();
    if let Err(err) = close_result {
        tracing::error!(error = %err, "render_pdf: failed to close print window");
    }
    result
}

/// Runs the actual WebView2 COM sequence via [`WebviewWindow::with_webview`] — Tauri's documented
/// raw-platform-handle escape hatch, which (like [`WebviewWindowBuilder::build`]) internally
/// dispatches its closure to the thread that actually owns the webview (Windows' real UI/event-loop
/// main thread, where WebView2's STA-affine COM objects must be called from) and is itself safe to
/// call from this function's own (background, `spawn_blocking`) thread. Everything the closure
/// needs to report back — just a `Result<(), String>`, never any of the raw COM/`windows-rs` types
/// themselves — crosses back out through a plain `mpsc` channel instead of the closure's own
/// (`()`-only) return value, since those COM types are not `Send` (required by `with_webview`'s own
/// `F: Send` bound) and must never need to cross this boundary in either direction.
fn run_print_to_pdf(window: &WebviewWindow, html: String, dest_path: &Path) -> Result<(), String> {
    let dest_path = dest_path.to_string_lossy().into_owned();
    // Cloned, not moved, into the `with_webview` closure below — this function's own outer copy is
    // still needed afterward, by `wait_for_file_ready`.
    let dest_path_for_closure = dest_path.clone();
    let (tx, rx) = mpsc::channel();
    window
        .with_webview(move |platform_webview| {
            tracing::info!("run_print_to_pdf: with_webview closure entered (now on main thread)");
            let result = print_to_pdf_com(platform_webview, &html, &dest_path_for_closure);
            tracing::info!(?result, "run_print_to_pdf: print_to_pdf_com finished");
            let _ = tx.send(result);
        })
        .map_err(|err| format!("failed to access webview controller: {err}"))?;
    tracing::info!("run_print_to_pdf: scheduled on main thread, waiting for result");
    let outcome = rx.recv().map_err(|_| "PDF render task was dropped before completion".to_string())?;
    tracing::info!(?outcome, "run_print_to_pdf: received result from main thread");
    outcome?;

    // `PrintToPdf`'s own completion handler firing "success" only means WebView2's print pipeline
    // (which runs the actual render/PDF encode in a separate process) has told the COM caller it's
    // done — it does not by itself guarantee that process's filesystem write is fully durable yet.
    // The caller (`render_pdf`) destroys this function's owning print window immediately once this
    // function returns, which can tear down that same process; a same-binary, same-code,
    // pass/fail-inconsistent-across-runs report (not a deterministic logic bug — the kind a wrong
    // path or malformed HTML would produce) is exactly the symptom of that race. Blocking here
    // (background `spawn_blocking` thread, never the main/UI thread) until the file is actually
    // observable with content closes that race at the only point this code can: before the window
    // gets torn down.
    wait_for_file_ready(&dest_path)
}

/// Polls for `dest_path` to exist with nonzero size, up to a few seconds total — see
/// [`run_print_to_pdf`]'s own doc comment for why this exists. Returns an error (surfaced to the
/// user via `pdf_export_write`'s `AppResult`, not silently dropped) if the file never appears in
/// time, rather than reporting success for an export that may not have actually produced a usable
/// file.
fn wait_for_file_ready(dest_path: &str) -> Result<(), String> {
    const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);
    const MAX_ATTEMPTS: u32 = 30; // 3 seconds total — generous relative to how fast this race, if it
                                   // is one, should resolve, but still bounded so a genuinely failed
                                   // export doesn't hang the command indefinitely.
    for attempt in 1..=MAX_ATTEMPTS {
        match std::fs::metadata(dest_path) {
            Ok(meta) if meta.len() > 0 => {
                tracing::info!(dest_path, size = meta.len(), attempt, "wait_for_file_ready: file ready");
                return Ok(());
            }
            Ok(meta) => {
                tracing::info!(dest_path, size = meta.len(), attempt, "wait_for_file_ready: file exists but still 0 bytes, retrying");
            }
            Err(err) => {
                tracing::info!(dest_path, attempt, error = %err, "wait_for_file_ready: file not yet visible, retrying");
            }
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    Err(format!(
        "PrintToPdf reported success but \"{dest_path}\" never appeared with content within {MAX_ATTEMPTS} x {POLL_INTERVAL:?}"
    ))
}

/// The actual WebView2 COM sequence, run from inside [`run_print_to_pdf`]'s `with_webview` closure
/// (so always on the correct thread — see that function's own doc comment). Gets the
/// `ICoreWebView2` controller Tauri's `wry` runtime wraps, navigates it directly to `html` via
/// `NavigateToString` (no `data:` URL encoding needed — this is a dedicated WebView2 API for
/// navigating to an HTML string in-memory), waits for that navigation to finish, then calls
/// `PrintToPdf`.
fn print_to_pdf_com(platform_webview: tauri::webview::PlatformWebview, html: &str, dest_path: &str) -> Result<(), String> {
    let controller: ICoreWebView2Controller = platform_webview.controller();
    let core = unsafe { controller.CoreWebView2() }.map_err(|err| format!("CoreWebView2 unavailable: {err}"))?;
    tracing::info!("print_to_pdf_com: got ICoreWebView2, navigating");

    navigate_and_wait(&core, html)?;
    tracing::info!("print_to_pdf_com: navigation completed successfully");

    // `PrintToPdf` was added in the `ICoreWebView2_7` minor-version interface (WebView2 Runtime
    // 1.0.1264.42+, shipped well before any currently-supported Windows 11 WebView2 Evergreen
    // Runtime build) — cast up from the base `ICoreWebView2`.
    let core7: ICoreWebView2_7 = core
        .cast()
        .map_err(|err| format!("ICoreWebView2_7 unavailable (WebView2 Runtime too old for PrintToPdf): {err}"))?;

    tracing::info!(dest_path, "print_to_pdf_com: calling PrintToPdf");
    let path_hstring = HSTRING::from(dest_path);
    let print_result = PrintToPdfCompletedHandler::wait_for_async_operation(
        Box::new(move |handler| unsafe { core7.PrintToPdf(&path_hstring, None, &handler) }.map_err(Into::into)),
        Box::new(|hresult_result: windows::core::Result<()>, success: bool| -> windows::core::Result<()> {
            tracing::info!(success, hresult_ok = hresult_result.is_ok(), "print_to_pdf_com: PrintToPdf completion handler invoked");
            hresult_result?;
            if success {
                Ok(())
            } else {
                Err(windows::core::Error::from(HRESULT(-1)))
            }
        }),
    );
    tracing::info!(?print_result, "print_to_pdf_com: wait_for_async_operation returned");
    print_result.map_err(|err| format!("PrintToPdf failed: {err}"))?;

    // Sanity check: `PrintToPdf`'s completion handler reporting success is documented to mean the
    // file was written, but logging whether it actually exists on disk (and its size) here removes
    // any doubt when diagnosing a "dialog appeared, nothing happened" report — cheap, and this is a
    // background thread already doing blocking I/O-adjacent work, not a hot path.
    match std::fs::metadata(dest_path) {
        Ok(meta) => tracing::info!(dest_path, size = meta.len(), "print_to_pdf_com: destination file exists after PrintToPdf"),
        Err(err) => tracing::error!(dest_path, error = %err, "print_to_pdf_com: destination file does NOT exist after PrintToPdf reported success"),
    }

    Ok(())
}

/// Navigates `core` directly to `html` (`NavigateToString` — no intermediate URL at all) and
/// blocks until `NavigationCompleted` fires, checking `IsSuccess` so a failed navigation surfaces
/// as an error instead of silently proceeding to print whatever half-loaded state resulted.
fn navigate_and_wait(core: &ICoreWebView2, html: &str) -> Result<(), String> {
    let (tx, rx) = mpsc::channel::<Result<(), String>>();
    let handler = NavigationCompletedEventHandler::create(Box::new(move |_sender, args| {
        let outcome = match args {
            Some(args) => {
                let mut success = BOOL(0);
                match unsafe { args.IsSuccess(&mut success) } {
                    Ok(()) if success.as_bool() => Ok(()),
                    Ok(()) => Err("navigation reported failure".to_string()),
                    Err(err) => Err(format!("IsSuccess failed: {err}")),
                }
            }
            None => Ok(()),
        };
        let _ = tx.send(outcome);
        Ok(())
    }));

    let mut token: i64 = 0;
    unsafe { core.add_NavigationCompleted(&handler, &mut token) }.map_err(|err| format!("add_NavigationCompleted failed: {err}"))?;

    let html_hstring = HSTRING::from(html);
    tracing::info!(html_len = html.len(), "navigate_and_wait: calling NavigateToString");
    unsafe { core.NavigateToString(&html_hstring) }.map_err(|err| format!("NavigateToString failed: {err}"))?;

    let outcome = webview2_com::wait_with_pump(rx).map_err(|err| format!("navigation wait failed: {err:?}"))?;
    tracing::info!(?outcome, "navigate_and_wait: NavigationCompleted fired");
    let _ = unsafe { core.remove_NavigationCompleted(token) };
    outcome
}
