import { useEffect, useState } from 'react';
import { EditorContent, useEditor, useEditorState } from '@tiptap/react';
import StarterKit from '@tiptap/starter-kit';
import TaskList from '@tiptap/extension-task-list';
import TaskItem from '@tiptap/extension-task-item';
import { Button } from '../ui';
import type { usePageSave } from './autosave';
import { exportPage, type Item, type Document } from './model';
export function PageEditor({
  item,
  history,
  saved,
  saving,
}: {
  item: Item;
  history: () => void;
  saved: () => void;
  saving: ReturnType<typeof usePageSave>;
}) {
  const [title, setTitle] = useState(item.title);
  const editor = useEditor({
    extensions: [
      StarterKit.configure({ heading: { levels: [1, 2, 3] }, link: { openOnClick: false } }),
      TaskList,
      TaskItem.configure({ nested: true }),
    ],
    content: item.document,
    immediatelyRender: false,
    editable: item.editable,
    editorProps: {
      attributes: {
        class: 'library-prose',
        role: 'textbox',
        'aria-label': 'Page content',
        'aria-multiline': 'true',
      },
    },
    onUpdate: ({ editor }) => saving.change(title, editor.getJSON() as Document),
  });
  // setEditable does not mutate document content or initiate a server write.
  useEffect(() => {
    editor?.setEditable(item.editable, false);
  }, [editor, item.editable]);
  const active = useEditorState({
    editor,
    selector: ({ editor }) => ({
      bold: editor?.isActive('bold'),
      italic: editor?.isActive('italic'),
      heading: editor?.isActive('heading'),
      list: editor?.isActive('bulletList'),
      tasks: editor?.isActive('taskList'),
    }),
  });
  return (
    <div className="library-writing">
      <div className="library-save-bar">
        <span role="status">
          {item.editable ? saving.status : 'Can view'} · Revision {saving.revision}
        </span>
        <div>
          <Button
            onClick={() =>
              exportPage({
                ...item,
                title,
                document: (editor?.getJSON() as Document) ?? item.document,
              })
            }
          >
            Export
          </Button>
          {item.editable && (
            <>
              <Button disabled={saving.dirty || saving.busy} onClick={history}>
                History
              </Button>
              <Button
                primary
                disabled={!saving.dirty || saving.busy || saving.blocked}
                onClick={() => void saving.save()}
              >
                Save
              </Button>
            </>
          )}
        </div>
      </div>
      {saving.blocked && (
        <p className="library-notice" role="alert">
          Your draft is kept in this browser. Copy or export it before loading another version.{' '}
          <Button onClick={saved}>Reload saved version</Button>
        </p>
      )}
      {saving.recovery && item.editable && (
        <div className="library-notice">
          <p>
            A draft from this browser is available. Recovering it will keep the current server
            revision as the starting point.
          </p>
          <Button
            onClick={() => {
              const draft = saving.recovery!;
              setTitle(draft.title);
              editor?.commands.setContent(draft.document, { emitUpdate: false });
              saving.change(draft.title, draft.document);
              saving.recovered();
            }}
          >
            Recover draft
          </Button>
          <Button onClick={saving.dismissRecovery}>Discard local draft</Button>
        </div>
      )}
      <input
        className="library-page-title"
        aria-label="Page title"
        value={title}
        readOnly={!item.editable}
        maxLength={180}
        onChange={(event) => {
          setTitle(event.target.value);
          saving.change(event.target.value, (editor?.getJSON() as Document) ?? item.document);
        }}
      />
      {item.editable && (
        <div className="library-toolbar" role="toolbar" aria-label="Text formatting">
          <Button
            aria-label="Bold"
            aria-pressed={!!active?.bold}
            onClick={() => editor?.chain().focus().toggleBold().run()}
          >
            <b>B</b>
          </Button>
          <Button
            aria-label="Italic"
            aria-pressed={!!active?.italic}
            onClick={() => editor?.chain().focus().toggleItalic().run()}
          >
            <i>I</i>
          </Button>
          <Button
            aria-label="Heading"
            aria-pressed={!!active?.heading}
            onClick={() => editor?.chain().focus().toggleHeading({ level: 2 }).run()}
          >
            H2
          </Button>
          <Button
            aria-label="Bullet list"
            aria-pressed={!!active?.list}
            onClick={() => editor?.chain().focus().toggleBulletList().run()}
          >
            • List
          </Button>
          <Button
            aria-label="Checklist"
            aria-pressed={!!active?.tasks}
            onClick={() => editor?.chain().focus().toggleTaskList().run()}
          >
            ☑ Checklist
          </Button>
          <Button aria-label="Undo" onClick={() => editor?.chain().focus().undo().run()}>
            ↶
          </Button>
          <Button aria-label="Redo" onClick={() => editor?.chain().focus().redo().run()}>
            ↷
          </Button>
        </div>
      )}
      <EditorContent editor={editor} />
      <p className="library-editor-hint">
        {item.editable
          ? 'Write a plan, keep useful instructions, or capture a decision. Changes save automatically.'
          : 'This page is shared with you. Contact its creator for edit access.'}
      </p>
    </div>
  );
}
