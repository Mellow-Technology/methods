import type { Extensions, JSONContent } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";

/**
 * Rich text in Methods is stored as TipTap's JSON document tree (the
 * ProseMirror document), never as HTML. `null` means empty.
 *
 * The server validates documents against the same schema
 * (server/src/richtext.rs): if you add a node or mark here, allow it there.
 */
export type RichDoc = JSONContent;

const SAFE_URL = /^(https?:|mailto:)/i;

/** Only web and mail links; everything else (javascript:, data:, …) is refused. */
export function isSafeUrl(url: string) {
  return SAFE_URL.test(url.trim());
}

/** Turn what someone typed into a link target: bare domains get https://. */
export function normaliseUrl(input: string): string | null {
  const url = input.trim();
  if (!url) return null;
  if (isSafeUrl(url)) return url;
  if (/^[a-z][a-z0-9+.-]*:/i.test(url)) return null; // some other scheme
  if (/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(url)) return `mailto:${url}`;
  return `https://${url}`;
}

/** The editor's schema, shared by the editor and the read-only renderer. */
export const richTextExtensions: Extensions = [
  StarterKit.configure({
    heading: { levels: [2, 3] },
    link: {
      openOnClick: false,
      autolink: true,
      defaultProtocol: "https",
      isAllowedUri: (url) => isSafeUrl(url),
      HTMLAttributes: { rel: "noopener noreferrer nofollow", target: "_blank" },
    },
  }),
];

/** True when a document has nothing worth keeping (just empty paragraphs). */
export function isEmptyDoc(doc: RichDoc | null | undefined): boolean {
  if (!doc) return true;
  const visit = (n: JSONContent): boolean => {
    if (n.type === "text") return !(n.text ?? "").trim();
    if (n.type === "horizontalRule") return false;
    return (n.content ?? []).every(visit);
  };
  return visit(doc);
}

/**
 * The document as it should be stored: null when empty, and without the
 * empty paragraphs TipTap keeps at the end for the caret (the server
 * trims them too).
 */
export function storedDoc(doc: RichDoc): RichDoc | null {
  if (isEmptyDoc(doc)) return null;
  const content = [...(doc.content ?? [])];
  while (content.length && content.at(-1)!.type === "paragraph" && !content.at(-1)!.content?.length) content.pop();
  return { ...doc, content };
}
