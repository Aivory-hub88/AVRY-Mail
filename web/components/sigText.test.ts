// Run: npm test  (Node's built-in runner)
import { test } from "node:test";
import assert from "node:assert/strict";
import { sigToText } from "./sigText.ts";

test("decodes &amp; back to a literal & (the reported bug)", () => {
  // What a contentEditable div's innerHTML actually looks like after
  // typing "CEO & FOUNDER" — browsers serialize "&" as "&amp;". Plain-text
  // is the default compose mode, so this hit every new email.
  assert.equal(
    sigToText("Best Regards,<br><br>Irfan Reichmann<br><br>CEO &amp; FOUNDER"),
    "Best Regards,\n\nIrfan Reichmann\n\nCEO & FOUNDER",
  );
});

test("decodes the other named entities and &nbsp;", () => {
  assert.equal(
    sigToText("a &lt;tag&gt; &amp; &quot;quotes&quot; &amp; it&#39;s&nbsp;fine"),
    'a <tag> & "quotes" & it\'s fine',
  );
});

test("decodes numeric and hex entities", () => {
  assert.equal(sigToText("&#38; and &#x26;"), "& and &");
});

test("never re-introduces markup: an entity spelling out a tag stays inert text", () => {
  // &lt;br&gt; must become the literal text "<br>", not an actual line break.
  assert.equal(sigToText("before&lt;br&gt;after"), "before<br>after");
});

test("leaves plain ampersands and unknown entities alone", () => {
  assert.equal(sigToText("Fish & Chips"), "Fish & Chips");
  assert.equal(sigToText("Q&amp;A &notareal; done"), "Q&A &notareal; done");
});

test("still collapses <br> to newlines and blank runs, still trims", () => {
  assert.equal(sigToText("  <p>a</p><p>b</p><br><br><br>c  "), "a\n\nb\n\nc");
});
