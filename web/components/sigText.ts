/**
 * Signature HTML <-> plain text, in its own module (no JSX) so it can run
 * under Node's test runner directly, same as newMailNotify.ts.
 */

/**
 * Decode HTML entities (`&amp;`, `&lt;`, numeric `&#38;` / `&#x26;`, ...)
 * back to plain characters. Runs after tags are already stripped, so this
 * never re-introduces markup — only literal text a contentEditable div (or
 * a pasted signature) encoded as an entity, most commonly a plain "&".
 */
const NAMED_ENTITIES: Record<string, string> = {
  amp: "&", lt: "<", gt: ">", quot: '"', apos: "'", nbsp: " ",
};
function decodeEntities(s: string): string {
  return s.replace(/&(#x[0-9a-f]+|#\d+|[a-z]+);/gi, (m, body: string) => {
    if (body[0] === "#") {
      const code = body[1] === "x" || body[1] === "X" ? parseInt(body.slice(2), 16) : parseInt(body.slice(1), 10);
      return Number.isFinite(code) ? String.fromCodePoint(code) : m;
    }
    return NAMED_ENTITIES[body.toLowerCase()] ?? m;
  });
}

/**
 * Plain-text twin of signature HTML (kept in sync on every save).
 *
 * A literal "&" typed into the editor is correctly stored in the HTML as
 * `&amp;` (that's just what innerHTML serializes it to) — this used to
 * strip tags without decoding it back, so the plain-text signature (and
 * anything built from it: plain-text-mode compose — the default compose
 * mode — and a message's body_text fallback when it has no body_html)
 * showed the literal characters "&amp;" instead of "&".
 */
export function sigToText(html: string): string {
  return decodeEntities(
    html
      .replace(/<br\s*\/?>/gi, "\n")
      .replace(/<\/(p|div|li|h[1-6])>\s*<(p|div|li|h[1-6])[^>]*>/gi, "\n\n")
      .replace(/<[^>]+>/g, "")
  )
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}
