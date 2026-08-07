// Sanitization allowlist (`docs/plans/markdown-editing/PLAN.md` §5; `[spec §4.2]`;
// `[detailed-spec §5.10]`). Enforced at the parser level: `markdownSpanParser.ts` calls the helpers
// below directly while it parses Markdown source into decoration attributes, applied on EVERY
// render pass (not just at save) — the underlying CodeMirror buffer/saved `.md` file always keeps
// the user's original typed text verbatim, this module only ever touches attributes destined for
// the rendered-DOM side. The decoration engine never builds an HTML string and hands it to the DOM;
// it constructs CodeMirror `Decoration` marks/widgets directly from these individually-validated
// attributes, so there is no HTML fragment for a sanitizer to run over in the first place.
//
// Allowlist (exact, no more/no less):
//   <img>  — src (relative path only; reject absolute paths, `file://`, any other scheme, and any
//            path containing a `..` segment — `DECISIONS.md` "Image `src` Allowlist: Reject `..`
//            Path-Traversal Segments"), width, height.
//   <span> — style, and within it only the `color` and `font-size` properties (any other property
//            in the same style string is stripped from that string, not the whole `<span>`).
// Everything else (`<script>`, any `on*` handler, `javascript:` URLs, `<iframe>`, `<style>`,
// disallowed CSS like `position`, etc.) is never emitted as a decoration attribute in the first
// place, since only `src`/`width`/`height`/`style` are ever read out of the parsed Markdown.

/** The only two CSS properties ever allowed inside an allowlisted `<span style="...">`. */
const ALLOWED_STYLE_PROPS = new Set(["color", "font-size"]);

/** Strips every CSS declaration from `style` except `color`/`font-size` (case-insensitive
 * property names). Does not further validate the *values* of the allowed properties — `color` and
 * `font-size` do not accept `url(...)` or executable content, so no additional value-level
 * scanning is needed for this narrow allowlist. */
export function filterStyleDeclarations(style: string): string {
  return style
    .split(";")
    .map((decl) => decl.trim())
    .filter(Boolean)
    .filter((decl) => {
      const colon = decl.indexOf(":");
      if (colon === -1) return false;
      const prop = decl.slice(0, colon).trim().toLowerCase();
      return ALLOWED_STYLE_PROPS.has(prop);
    })
    .join("; ");
}

/** `true` if `src` is safe under the allowlist: relative (no scheme, no leading `/`), and contains
 * no `..` path-traversal segment. Rejects `file://`, `http(s)://`, `javascript:`, protocol-relative
 * `//host/...`, absolute Windows/POSIX paths, and any segment equal to `..`. */
export function isSafeRelativeImageSrc(src: string): boolean {
  const trimmed = src.trim();
  if (!trimmed) return false;
  // Any `scheme:` prefix (including `javascript:`, `file:`, `data:`, `http:`) is rejected — only a
  // bare relative reference is allowed.
  if (/^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(trimmed)) return false;
  if (trimmed.startsWith("//")) return false; // protocol-relative
  if (trimmed.startsWith("/") || /^[a-zA-Z]:[\\/]/.test(trimmed)) return false; // absolute path
  const segments = trimmed.split(/[\\/]/);
  if (segments.some((seg) => seg === "..")) return false;
  return true;
}
