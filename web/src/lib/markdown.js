import { Marked } from "marked";

const escapeHtml = (s) =>
  s.replace(
    /[&<>"']/g,
    (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c],
  );

// Discord's `-# text` subtext line; without this marked reads it as a list item holding a heading.
const subtext = {
  name: "subtext",
  level: "block",
  start: (src) => src.match(/^-# /m)?.index,
  tokenizer(src) {
    const match = /^-# +([^\n]*)(?:\n|$)/.exec(src);
    if (match) return { type: "subtext", raw: match[0], tokens: this.lexer.inlineTokens(match[1]) };
  },
  renderer(token) {
    return `<p class="subtext">${this.parser.parseInline(token.tokens)}</p>\n`;
  },
};

// Discord-flavoured inline syntax on top of GFM.
function wrapped(name, delimiter, tag) {
  const rule = new RegExp(`^${delimiter}([\\s\\S]+?)${delimiter}`);
  return {
    name,
    level: "inline",
    start: (src) => src.indexOf(delimiter.replaceAll("\\", "")),
    tokenizer(src) {
      const match = rule.exec(src);
      if (match) return { type: name, raw: match[0], tokens: this.lexer.inlineTokens(match[1]) };
    },
    renderer(token) {
      return `${tag[0]}${this.parser.parseInline(token.tokens)}${tag[1]}`;
    },
  };
}

const marked = new Marked({
  gfm: true,
  breaks: true,
  extensions: [
    subtext,
    wrapped("spoiler", "\\|\\|", ['<span class="spoiler">', "</span>"]),
    wrapped("underline", "__", ["<u>", "</u>"]),
  ],
  renderer: {
    // Posts are user content: raw HTML is shown as text.
    html: ({ text }) => escapeHtml(text),
  },
});

export const renderMarkdown = (src) => marked.parse(src);
