"use client";

import { forwardRef, useEffect, useImperativeHandle, useMemo, useRef, useState } from "react";
import { EditorContent, useEditor, useEditorState, type Editor } from "@tiptap/react";
import { Placeholder } from "@tiptap/extensions";
import { renderToReactElement } from "@tiptap/static-renderer/pm/react";
import { normaliseUrl, richTextExtensions, storedDoc, type RichDoc } from "@/lib/richtext";
import styles from "./RichTextEditor.module.scss";

export interface RichTextHandle {
  clear: () => void;
  focus: () => void;
  /** The current document, or null when it's empty. */
  getDoc: () => RichDoc | null;
}

interface Props {
  value?: RichDoc | null;
  onChange?: (doc: RichDoc | null) => void;
  /** Leaving the editor, only when its content changed since it was loaded. */
  onBlur?: (doc: RichDoc | null) => void;
  /** ⌘/Ctrl+Enter. */
  onSubmit?: () => void;
  placeholder?: string;
  label: string;
  minHeight?: string;
  className?: string;
}

const docOf = (editor: Editor): RichDoc | null => storedDoc(editor.getJSON());

type Tool = {
  name: string;
  label: string;
  title: string;
  run: (e: Editor) => void;
  active: (e: Editor) => boolean;
};

const TOOLS: Tool[] = [
  { name: "bold", label: "B", title: "Bold (⌘B)", run: (e) => e.chain().focus().toggleBold().run(), active: (e) => e.isActive("bold") },
  { name: "italic", label: "I", title: "Italic (⌘I)", run: (e) => e.chain().focus().toggleItalic().run(), active: (e) => e.isActive("italic") },
  { name: "strike", label: "S", title: "Strikethrough (⌘⇧S)", run: (e) => e.chain().focus().toggleStrike().run(), active: (e) => e.isActive("strike") },
  { name: "code", label: "`", title: "Inline code (⌘E)", run: (e) => e.chain().focus().toggleCode().run(), active: (e) => e.isActive("code") },
  { name: "heading", label: "H", title: "Heading", run: (e) => e.chain().focus().toggleHeading({ level: 3 }).run(), active: (e) => e.isActive("heading") },
  { name: "bulletList", label: "•", title: "Bulleted list", run: (e) => e.chain().focus().toggleBulletList().run(), active: (e) => e.isActive("bulletList") },
  { name: "orderedList", label: "1.", title: "Numbered list", run: (e) => e.chain().focus().toggleOrderedList().run(), active: (e) => e.isActive("orderedList") },
  { name: "blockquote", label: "❝", title: "Quote", run: (e) => e.chain().focus().toggleBlockquote().run(), active: (e) => e.isActive("blockquote") },
  { name: "codeBlock", label: "</>", title: "Code block", run: (e) => e.chain().focus().toggleCodeBlock().run(), active: (e) => e.isActive("codeBlock") },
];

/**
 * Rich text editing on TipTap. Values are TipTap's JSON document (see
 * lib/richtext.ts); null means empty.
 *
 * Interim until gui-mellow ships its own RichTextEditor (GM-13).
 */
export const RichTextEditor = forwardRef<RichTextHandle, Props>(function RichTextEditor(
  { value = null, onChange, onBlur, onSubmit, placeholder, label, minHeight = "6rem", className },
  ref,
) {
  // Keep the latest callbacks without re-creating the editor.
  const callbacks = useRef({ onChange, onBlur, onSubmit });
  callbacks.current = { onChange, onBlur, onSubmit };
  const wrapper = useRef<HTMLDivElement>(null);
  // The content as last loaded or reported, in the editor's own JSON. Stored
  // documents can't be compared directly: the database reorders keys and the
  // server drops default attributes.
  const baseline = useRef("");

  const editor = useEditor({
    // Static export: render the editor only in the browser.
    immediatelyRender: false,
    extensions: [...richTextExtensions, Placeholder.configure({ placeholder: placeholder ?? "" })],
    content: value ?? "",
    editorProps: {
      attributes: {
        class: `${styles.prose} ${styles.content}`,
        role: "textbox",
        "aria-multiline": "true",
        "aria-label": label,
        style: `min-height: ${minHeight}`,
      },
      handleKeyDown: (_view, event) => {
        if (event.key === "Enter" && (event.metaKey || event.ctrlKey) && callbacks.current.onSubmit) {
          event.preventDefault();
          callbacks.current.onSubmit();
          return true;
        }
        return false;
      },
    },
    onCreate: ({ editor }) => {
      baseline.current = JSON.stringify(docOf(editor));
    },
    onUpdate: ({ editor }) => callbacks.current.onChange?.(docOf(editor)),
    onBlur: ({ editor, event }) => {
      // Moving into our own toolbar or link field isn't leaving the editor.
      if (event.relatedTarget instanceof Node && wrapper.current?.contains(event.relatedTarget)) return;
      const doc = docOf(editor);
      const json = JSON.stringify(doc);
      if (json === baseline.current) return;
      baseline.current = json;
      callbacks.current.onBlur?.(doc);
    },
  });

  // Take in outside changes (a different item, a saved version), but never
  // while someone is typing: that would move their caret.
  useEffect(() => {
    if (!editor || editor.isFocused) return;
    editor.commands.setContent(value ?? "", { emitUpdate: false });
    baseline.current = JSON.stringify(docOf(editor));
  }, [editor, value]);

  useImperativeHandle(
    ref,
    () => ({
      clear: () => {
        editor?.commands.clearContent(true);
      },
      focus: () => editor?.commands.focus("end"),
      getDoc: () => (editor ? docOf(editor) : null),
    }),
    [editor],
  );

  const active = useEditorState({
    editor,
    selector: ({ editor }) =>
      editor
        ? Object.fromEntries([...TOOLS.map((t) => [t.name, t.active(editor)]), ["link", editor.isActive("link")]])
        : {},
  });

  const [linkDraft, setLinkDraft] = useState<string | null>(null);
  const [linkError, setLinkError] = useState(false);

  function openLink() {
    if (!editor) return;
    setLinkError(false);
    setLinkDraft(editor.getAttributes("link").href ?? "");
  }

  function applyLink() {
    if (!editor || linkDraft === null) return;
    if (!linkDraft.trim()) {
      editor.chain().focus().extendMarkRange("link").unsetLink().run();
      setLinkDraft(null);
      return;
    }
    const href = normaliseUrl(linkDraft);
    if (!href) {
      setLinkError(true);
      return;
    }
    const chain = editor.chain().focus().extendMarkRange("link");
    if (editor.state.selection.empty && !editor.isActive("link")) {
      // Nothing selected: insert the address itself as the link text.
      chain.insertContent({ type: "text", text: linkDraft.trim(), marks: [{ type: "link", attrs: { href } }] }).run();
    } else {
      chain.setLink({ href }).run();
    }
    setLinkDraft(null);
  }

  return (
    <div ref={wrapper} className={[styles.editor, className].filter(Boolean).join(" ")}>
      <div className={styles.toolbar} role="toolbar" aria-label={`${label} formatting`}>
        {TOOLS.map((t) => (
          <button
            key={t.name}
            type="button"
            title={t.title}
            aria-label={t.title}
            aria-pressed={!!active?.[t.name]}
            className={styles.tool}
            disabled={!editor}
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => editor && t.run(editor)}
          >
            {t.label}
          </button>
        ))}
        <button
          type="button"
          title="Link (⌘K)"
          aria-label="Link"
          aria-pressed={!!active?.link}
          aria-expanded={linkDraft !== null}
          className={styles.tool}
          disabled={!editor}
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => (linkDraft === null ? openLink() : setLinkDraft(null))}
        >
          🔗
        </button>
      </div>

      {linkDraft !== null && (
        <form
          className={styles.linkRow}
          onSubmit={(e) => {
            e.preventDefault();
            applyLink();
          }}
        >
          <input
            autoFocus
            className={styles.linkInput}
            aria-label="Link address"
            aria-invalid={linkError || undefined}
            placeholder="https://… or someone@example.com"
            value={linkDraft}
            onChange={(e) => {
              setLinkDraft(e.target.value);
              setLinkError(false);
            }}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                e.preventDefault();
                setLinkDraft(null);
                editor?.commands.focus();
              }
            }}
          />
          <button type="submit" className={styles.linkButton}>
            Apply
          </button>
          {active?.link && (
            <button
              type="button"
              className={styles.linkButton}
              onClick={() => {
                editor?.chain().focus().extendMarkRange("link").unsetLink().run();
                setLinkDraft(null);
              }}
            >
              Remove
            </button>
          )}
          {linkError && <span className={styles.linkError}>Only web and email links</span>}
        </form>
      )}

      <EditorContent
        editor={editor}
        onKeyDown={(e) => {
          if (e.key === "k" && (e.metaKey || e.ctrlKey)) {
            e.preventDefault();
            openLink();
          }
        }}
      />
    </div>
  );
});

/** A stored document, rendered read-only without starting an editor. */
export function RichText({ doc, className }: { doc: RichDoc | null; className?: string }) {
  const content = useMemo(
    () => (doc ? renderToReactElement({ content: doc, extensions: richTextExtensions }) : null),
    [doc],
  );
  return <div className={[styles.prose, className].filter(Boolean).join(" ")}>{content}</div>;
}
