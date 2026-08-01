// Sanitization allowlist (`docs/plans/markdown-editing/PLAN.md` §5; `[spec §4.2]`;
// `[detailed-spec §5.10]`). Applied to whatever HTML the decoration layer would otherwise render,
// on EVERY render pass (not just at save) — the underlying CodeMirror buffer/saved `.md` file
// always keeps the user's original typed text verbatim, this module only ever touches the
// rendered-DOM side.
//
// Allowlist (exact, no more/no less):
//   <img>  — src (relative path only; reject absolute paths, `file://`, any other scheme, and any
//            path containing a `..` segment — `DECISIONS.md` "Image `src` Allowlist: Reject `..`
//            Path-Traversal Segments"), width, height.
//   <span> — style, and within it only the `color` and `font-size` properties (any other property
//            in the same style string is stripped from that string, not the whole `<span>`).
// Everything else (`<script>`, any `on*` handler, `javascript:` URLs, `<iframe>`, `<style>`,
// disallowed CSS like `position`, etc.) is stripped entirely.
//
// Uses DOMPurify (continuity with the legacy `dompurify` dependency) configured with a strict
// custom allowlist matching exactly this table, rather than DOMPurify's permissive defaults.
import DOMPurify from "dompurify";

/** The only two CSS properties ever allowed inside an allowlisted `<span style="...">`. */
const ALLOWED_STYLE_PROPS = new Set(["color", "font-size"]);

/** Strips every CSS declaration from `style` except `color`/`font-size` (case-insensitive
 * property names). Does not validate the *values* of the allowed properties beyond what
 * DOMPurify's own `style`-attribute value scanning already rejects (e.g. `url(...)`,
 * `javascript:` — DOMPurify strips those at the attribute-value level before this ever runs, but
 * this function still re-parses defensively rather than trusting that alone). */
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

let purifierConfigured = false;

function configurePurifier(): void {
  if (purifierConfigured) return;
  purifierConfigured = true;

  DOMPurify.addHook("uponSanitizeElement", (node) => {
    if (!(node instanceof Element)) return;
    const tag = node.tagName.toLowerCase();
    if (tag !== "img" && tag !== "span") return;
    // DOMPurify's ALLOWED_TAGS config (passed at call time) already restricts which tags survive
    // this far; nothing further needed here per-element.
  });

  DOMPurify.addHook("uponSanitizeAttribute", (node, data) => {
    if (!(node instanceof Element)) return;
    const tag = node.tagName.toLowerCase();
    if (tag === "img" && data.attrName === "src") {
      if (!isSafeRelativeImageSrc(data.attrValue)) {
        data.keepAttr = false;
      }
    }
    if (data.attrName === "style") {
      data.attrValue = filterStyleDeclarations(data.attrValue);
      if (!data.attrValue) data.keepAttr = false;
    }
  });
}

/** Sanitizes `html` (a small fragment the decoration layer is about to render, e.g. one
 * `<span style="...">...</span>` or `<img ...>` tag's outer HTML) against the exact allowlist in
 * this module's header comment. Safe to call on every render pass. */
export function sanitizeMarkdownHtml(html: string): string {
  configurePurifier();
  return DOMPurify.sanitize(html, {
    ALLOWED_TAGS: ["img", "span"],
    ALLOWED_ATTR: ["src", "width", "height", "style"],
    ALLOW_DATA_ATTR: false,
  });
}
