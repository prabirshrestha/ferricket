import { useEffect, useId, useMemo, useRef, useState } from "react"
import type { Editor } from "@tiptap/core"
import { EditorContent, useEditor } from "@tiptap/react"
import StarterKit from "@tiptap/starter-kit"
import { Markdown } from "@tiptap/markdown"
import TaskList from "@tiptap/extension-task-list"
import TaskItem from "@tiptap/extension-task-item"
import Placeholder from "@tiptap/extension-placeholder"
import Image from "@tiptap/extension-image"
import {
  Bold,
  Check,
  Code,
  Code2,
  Heading1,
  Heading2,
  Heading3,
  Italic,
  List,
  ListChecks,
  ListOrdered,
  Minus,
  LoaderCircle,
  Paperclip,
  Pilcrow,
  Quote,
  Save,
  Strikethrough,
} from "lucide-react"
import { Button } from "../../components/ui/button"
import { Textarea } from "../../components/ui/textarea"
import { Tooltip, TooltipContent, TooltipTrigger } from "../../components/ui/tooltip"
import { cn } from "../../lib/utils"
import type { Ticket } from "../../types/ticket"
import { uploadAttachment } from "../../api/tickets"
import {
  attachmentMarkdownForEditor,
  attachmentMarkdownForStorage,
  previewableImageTypes,
  validateAttachments,
} from "./editor-attachments"
import { EditorSuggestionMenu } from "./editor-suggestion-menu"
import { EditorSlashMenu, type SlashMenuState } from "./editor-slash-menu"
import { useEditorSuggestions } from "./use-editor-suggestions"

type Mode = "rich" | "raw"
type SlashRange = { from: number; to: number }
type SlashCommand = {
  label: string
  description: string
  keywords: string
  icon: typeof Heading2
  run: (editor: Editor, range: SlashRange) => void
}

const deleteSlash = (editor: Editor, range: SlashRange) => editor.chain().focus().deleteRange(range)
const slashCommands: SlashCommand[] = [
  {
    label: "Text",
    description: "Start writing with plain text.",
    keywords: "paragraph body",
    icon: Pilcrow,
    run: (editor, range) => deleteSlash(editor, range).setParagraph().run(),
  },
  {
    label: "Heading 1",
    description: "Large section heading.",
    keywords: "title h1",
    icon: Heading1,
    run: (editor, range) => deleteSlash(editor, range).setHeading({ level: 1 }).run(),
  },
  {
    label: "Heading 2",
    description: "Medium section heading.",
    keywords: "title h2",
    icon: Heading2,
    run: (editor, range) => deleteSlash(editor, range).setHeading({ level: 2 }).run(),
  },
  {
    label: "Heading 3",
    description: "Small section heading.",
    keywords: "title h3",
    icon: Heading3,
    run: (editor, range) => deleteSlash(editor, range).setHeading({ level: 3 }).run(),
  },
  {
    label: "Bulleted list",
    description: "Create a simple bulleted list.",
    keywords: "bullet unordered",
    icon: List,
    run: (editor, range) => deleteSlash(editor, range).toggleBulletList().run(),
  },
  {
    label: "Numbered list",
    description: "Create a list with numbering.",
    keywords: "ordered number",
    icon: ListOrdered,
    run: (editor, range) => deleteSlash(editor, range).toggleOrderedList().run(),
  },
  {
    label: "To-do list",
    description: "Track work with checkboxes.",
    keywords: "task checklist checkbox",
    icon: ListChecks,
    run: (editor, range) => deleteSlash(editor, range).toggleTaskList().run(),
  },
  {
    label: "Quote",
    description: "Capture a quotation.",
    keywords: "blockquote",
    icon: Quote,
    run: (editor, range) => deleteSlash(editor, range).toggleBlockquote().run(),
  },
  {
    label: "Code block",
    description: "Write a formatted code snippet.",
    keywords: "preformatted",
    icon: Code2,
    run: (editor, range) => deleteSlash(editor, range).toggleCodeBlock().run(),
  },
  {
    label: "Divider",
    description: "Visually separate sections.",
    keywords: "horizontal rule separator",
    icon: Minus,
    run: (editor, range) => deleteSlash(editor, range).setHorizontalRule().run(),
  },
]

function runSlash(editor: Editor, command: SlashCommand) {
  const { selection } = editor.state
  if (!selection.empty || !selection.$from.parent.isTextblock) return false
  const textBefore = selection.$from.parent.textBetween(
    0,
    selection.$from.parentOffset,
    undefined,
    "\ufffc",
  )
  const match = slashQuery(textBefore)
  if (!match) return false
  command.run(editor, { from: selection.from - match.query.length - 1, to: selection.from })
  return true
}

export function slashQuery(textBeforeCaret: string) {
  const match = textBeforeCaret.match(/(?:^|\s)\/([^\s/]*)$/)
  if (!match) return null
  return { query: match[1] ?? "", startOffset: textBeforeCaret.lastIndexOf("/") }
}

type ComposerProps = {
  editorKey: string
  value: string
  onChange: (value: string) => void
  savedValue?: string
  onSave?: (description: string) => Promise<void>
  compact?: boolean
  ticketId?: string
  tickets?: Ticket[]
  currentUser?: string
  stagedAttachments?: File[]
  onStageAttachments?: (files: File[]) => void
  onSelect?: (id: string) => void
  onError?: (message: string) => void
}

export function MarkdownEditor({
  ticket,
  tickets,
  currentUser,
  onSave,
  onSelect,
  onError,
}: {
  ticket: Ticket
  tickets: Ticket[]
  currentUser?: string
  onSave: (description: string) => Promise<void>
  onSelect: (id: string) => void
  onError?: (message: string) => void
}) {
  const [value, setValue] = useState(ticket.description)
  useEffect(() => setValue(ticket.description), [ticket.description])
  return (
    <MarkdownComposer
      editorKey={ticket.id}
      value={value}
      onChange={setValue}
      savedValue={ticket.description}
      onSave={onSave}
      ticketId={ticket.id}
      tickets={tickets}
      currentUser={currentUser}
      onSelect={onSelect}
      onError={onError}
    />
  )
}

export function MarkdownComposer({
  editorKey,
  value,
  onChange,
  savedValue,
  onSave,
  compact = false,
  ticketId,
  tickets = [],
  currentUser,
  stagedAttachments = [],
  onStageAttachments,
  onSelect,
  onError,
}: ComposerProps) {
  const [mode, setMode] = useState<Mode>("rich")
  const [saving, setSaving] = useState(false)
  const [slash, setSlash] = useState<SlashMenuState | null>(null)
  const [suggestionIndex, setSuggestionIndex] = useState(0)
  const [uploading, setUploading] = useState(0)
  const [attachmentError, setAttachmentError] = useState("")
  const fileInput = useRef<HTMLInputElement>(null)
  const suggestionMenuId = useId()
  const slashMenuId = useId()
  const insertFilesRef = useRef<(files: File[]) => void>(() => undefined)
  const slashRef = useRef<SlashMenuState | null>(null)
  const optionsRef = useRef<SlashCommand[]>(slashCommands)
  const suggestionIndexRef = useRef(suggestionIndex)
  suggestionIndexRef.current = suggestionIndex
  const saveRef = useRef<() => Promise<void>>(async () => undefined)
  const editorRef = useRef<Editor | null>(null)
  const selectRef = useRef(onSelect)
  selectRef.current = onSelect
  const editorSuggestions = useEditorSuggestions({ tickets, currentUser })

  const updateSlashMenu = (editor: Editor) => {
    const { selection } = editor.state
    if (!editor.isFocused || !selection.empty || !selection.$from.parent.isTextblock) {
      slashRef.current = null
      setSlash(null)
      return
    }
    const textBefore = selection.$from.parent.textBetween(
      0,
      selection.$from.parentOffset,
      undefined,
      "\ufffc",
    )
    const match = slashQuery(textBefore)
    if (!match) {
      slashRef.current = null
      setSlash(null)
      return
    }
    const coords = editor.view.coordsAtPos(selection.from)
    const next = {
      query: match.query,
      from: selection.from - match.query.length - 1,
      to: selection.from,
      left: Math.max(8, Math.min(coords.left, window.innerWidth - 304)),
      top:
        coords.bottom + 360 < window.innerHeight
          ? coords.bottom + 6
          : Math.max(8, coords.top - 356),
    }
    slashRef.current = next
    setSlash(next)
  }

  const updateEditorMenus = (editor: Editor) => {
    if (editorSuggestions.update(editor)) {
      slashRef.current = null
      setSlash(null)
    } else {
      updateSlashMenu(editor)
    }
  }

  const editor = useEditor(
    {
      extensions: [
        StarterKit.configure({ link: { autolink: true, linkOnPaste: true, openOnClick: true } }),
        TaskList,
        TaskItem.configure({ nested: true }),
        Image.configure({ HTMLAttributes: { class: "ticket-attachment-image" } }),
        Placeholder.configure({ placeholder: "Add a description… Type / for commands." }),
        Markdown,
      ],
      content: attachmentMarkdownForEditor(value),
      contentType: "markdown",
      immediatelyRender: true,
      shouldRerenderOnTransaction: true,
      onUpdate: ({ editor }) => {
        onChange(attachmentMarkdownForStorage(editor.getMarkdown()))
        updateEditorMenus(editor)
      },
      onSelectionUpdate: ({ editor }) => updateEditorMenus(editor),
      onFocus: ({ editor }) => updateEditorMenus(editor),
      onBlur: () => {
        slashRef.current = null
        setSlash(null)
        editorSuggestions.close()
      },
      editorProps: {
        attributes: { "aria-label": "Ticket description" },
        handleClick: (_view, _position, event) => {
          const anchor = event.target instanceof Element ? event.target.closest("a") : null
          const href = anchor?.getAttribute("href")
          const match = href?.match(/^\/all\/ticket\/([^/?#]+)/)
          if (!match || !selectRef.current) return false
          event.preventDefault()
          selectRef.current(decodeURIComponent(match[1]))
          return true
        },
        handleDrop: (_view, event) => {
          const files = [...(event.dataTransfer?.files ?? [])]
          if (!files.length) return false
          event.preventDefault()
          insertFilesRef.current(files)
          return true
        },
        handlePaste: (_view, event) => {
          const files = [...(event.clipboardData?.files ?? [])]
          if (!files.length) return false
          event.preventDefault()
          insertFilesRef.current(files)
          return true
        },
        handleKeyDown: (_view, event) => {
          if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
            event.preventDefault()
            void saveRef.current()
            return true
          }
          if (editorRef.current && editorSuggestions.handleKey(editorRef.current, event))
            return true
          const current = slashRef.current
          const options = optionsRef.current
          if (!current || !options.length) return false
          if (event.key === "ArrowDown" || event.key === "ArrowUp") {
            event.preventDefault()
            setSuggestionIndex(
              (index) =>
                (index + (event.key === "ArrowDown" ? 1 : options.length - 1)) % options.length,
            )
            return true
          }
          if (event.key === "Enter" || event.key === "Tab") {
            event.preventDefault()
            const command = options[suggestionIndexRef.current]
            if (command) {
              if (editorRef.current) runSlash(editorRef.current, command)
              slashRef.current = null
              setSlash(null)
            }
            return true
          }
          if (event.key === "Escape") {
            event.preventDefault()
            slashRef.current = null
            setSlash(null)
            return true
          }
          return false
        },
      },
    },
    [editorKey],
  )
  editorRef.current = editor

  useEffect(() => {
    if (!editor || editor.isFocused || attachmentMarkdownForStorage(editor.getMarkdown()) === value)
      return
    editor.commands.setContent(attachmentMarkdownForEditor(value), {
      contentType: "markdown",
      emitUpdate: false,
    })
  }, [editor, value])

  const filteredCommands = useMemo(() => {
    const query = slash?.query.trim().toLocaleLowerCase() ?? ""
    return slashCommands.filter(
      (command) =>
        !query || (command.label + " " + command.keywords).toLocaleLowerCase().includes(query),
    )
  }, [slash?.query])
  optionsRef.current = filteredCommands

  useEffect(() => {
    if (!editor) return
    const element = editor.view.dom
    const suggestionOpen = Boolean(editorSuggestions.state)
    const slashOpen = Boolean(slash)
    const menuId = suggestionOpen ? suggestionMenuId : slashOpen ? slashMenuId : null
    const activeIndex = suggestionOpen ? editorSuggestions.index : suggestionIndex
    const hasActiveOption = suggestionOpen
      ? Boolean(editorSuggestions.suggestions[activeIndex])
      : Boolean(filteredCommands[activeIndex])
    element.setAttribute("role", "combobox")
    element.setAttribute("aria-autocomplete", "list")
    element.setAttribute("aria-haspopup", "listbox")
    element.setAttribute("aria-expanded", String(Boolean(menuId)))
    if (menuId) element.setAttribute("aria-controls", menuId)
    else element.removeAttribute("aria-controls")
    if (menuId && hasActiveOption)
      element.setAttribute("aria-activedescendant", `${menuId}-option-${activeIndex}`)
    else element.removeAttribute("aria-activedescendant")
  }, [
    editor,
    editorSuggestions.index,
    editorSuggestions.state,
    editorSuggestions.suggestions,
    filteredCommands,
    slash,
    slashMenuId,
    suggestionIndex,
    suggestionMenuId,
  ])

  const runSlashCommand = (command: SlashCommand) => {
    if (!editor || !slashRef.current) return
    runSlash(editor, command)
    slashRef.current = null
    setSlash(null)
  }
  useEffect(() => setSuggestionIndex(0), [slash?.query])

  const currentMarkdown = () =>
    mode === "raw" ? value : attachmentMarkdownForStorage(editor?.getMarkdown() ?? value)
  const save = async () => {
    const markdown = currentMarkdown()
    if (!onSave) return
    setSaving(true)
    try {
      await onSave(markdown)
      onChange(markdown)
    } finally {
      setSaving(false)
    }
  }
  saveRef.current = save

  const switchMode = (next: Mode) => {
    if (next === mode) return
    if (next === "rich")
      editor?.commands.setContent(attachmentMarkdownForEditor(value), {
        contentType: "markdown",
        emitUpdate: false,
      })
    else if (editor) onChange(attachmentMarkdownForStorage(editor.getMarkdown()))
    setMode(next)
    slashRef.current = null
    setSlash(null)
    editorSuggestions.close()
  }

  const insertUploadedAttachment = (attachment: Awaited<ReturnType<typeof uploadAttachment>>) => {
    if (mode === "raw") {
      onChange([value, attachment.markdown].filter(Boolean).join("\n\n"))
      return
    }
    if (!editor) return
    if (previewableImageTypes.has(attachment.media_type)) {
      editor
        .chain()
        .focus()
        .setImage({ src: attachment.url, alt: attachment.name, title: attachment.name })
        .run()
    } else {
      editor
        .chain()
        .focus()
        .insertContent({
          type: "text",
          text: attachment.name,
          marks: [{ type: "link", attrs: { href: attachment.url, target: "_blank" } }],
        })
        .insertContent(" ")
        .run()
    }
  }

  const insertFiles = async (files: File[]) => {
    const invalid = validateAttachments(files)
    if (invalid) {
      setAttachmentError(invalid)
      onError?.(invalid)
      return
    }
    setAttachmentError("")
    if (!ticketId) {
      onStageAttachments?.([...stagedAttachments, ...files])
      return
    }
    setUploading((count) => count + files.length)
    try {
      const attachments = await Promise.all(files.map((file) => uploadAttachment(ticketId, file)))
      attachments.forEach(insertUploadedAttachment)
    } catch (reason) {
      const message = reason instanceof Error ? reason.message : String(reason)
      setAttachmentError(message)
      onError?.(message)
    } finally {
      setUploading((count) => Math.max(0, count - files.length))
    }
  }
  insertFilesRef.current = (files) => void insertFiles(files)

  return (
    <div className={compact ? "mt-1 flex flex-col" : "mt-5"}>
      <div
        className={cn(
          "flex min-h-9 items-center gap-0.5 overflow-x-auto py-1",
          compact ? "order-last border-t" : "border-b",
        )}
      >
        {mode === "rich" && editor && (
          <>
            <FormatButton
              label="Bold"
              active={editor.isActive("bold")}
              onClick={() => editor.chain().focus().toggleBold().run()}
            >
              <Bold />
            </FormatButton>
            <FormatButton
              label="Italic"
              active={editor.isActive("italic")}
              onClick={() => editor.chain().focus().toggleItalic().run()}
            >
              <Italic />
            </FormatButton>
            <FormatButton
              label="Strikethrough"
              active={editor.isActive("strike")}
              onClick={() => editor.chain().focus().toggleStrike().run()}
            >
              <Strikethrough />
            </FormatButton>
            <FormatButton
              label="Inline code"
              active={editor.isActive("code")}
              onClick={() => editor.chain().focus().toggleCode().run()}
            >
              <Code />
            </FormatButton>
            <span className="mx-1 h-4 w-px shrink-0 bg-border" />
            <FormatButton
              label="Heading"
              active={editor.isActive("heading", { level: 2 })}
              onClick={() => editor.chain().focus().toggleHeading({ level: 2 }).run()}
            >
              <Heading2 />
            </FormatButton>
            <FormatButton
              label="Bulleted list"
              active={editor.isActive("bulletList")}
              onClick={() => editor.chain().focus().toggleBulletList().run()}
            >
              <List />
            </FormatButton>
            <FormatButton
              label="Numbered list"
              active={editor.isActive("orderedList")}
              onClick={() => editor.chain().focus().toggleOrderedList().run()}
            >
              <ListOrdered />
            </FormatButton>
            <FormatButton
              label="To-do list"
              active={editor.isActive("taskList")}
              onClick={() => editor.chain().focus().toggleTaskList().run()}
            >
              <ListChecks />
            </FormatButton>
            <FormatButton
              label="Quote"
              active={editor.isActive("blockquote")}
              onClick={() => editor.chain().focus().toggleBlockquote().run()}
            >
              <Quote />
            </FormatButton>
          </>
        )}
        <div className="ml-auto flex shrink-0 items-center gap-1 pl-3">
          <input
            ref={fileInput}
            type="file"
            multiple
            className="sr-only"
            aria-label="Attach files"
            onChange={(event) => {
              const files = [...(event.target.files ?? [])]
              if (files.length) void insertFiles(files)
              event.target.value = ""
            }}
          />
          <Button
            type="button"
            variant="ghost"
            size="sm"
            disabled={uploading > 0}
            onClick={() => fileInput.current?.click()}
          >
            {uploading > 0 ? <LoaderCircle className="animate-spin" /> : <Paperclip />}
            {uploading > 0 ? `Uploading ${uploading}` : "Attach"}
          </Button>
          <Button
            variant={mode === "raw" ? "secondary" : "ghost"}
            size="sm"
            aria-pressed={mode === "raw"}
            onClick={() => switchMode(mode === "raw" ? "rich" : "raw")}
          >
            <Code2 /> Raw
          </Button>
          {onSave && (
            <Button
              variant="ghost"
              size="sm"
              disabled={saving || currentMarkdown() === savedValue}
              onClick={() => void save()}
            >
              {saving ? <Check /> : <Save />}
              {saving ? "Saving…" : "Save"}
            </Button>
          )}
        </div>
      </div>
      {stagedAttachments.length > 0 && (
        <div className="flex flex-wrap gap-1.5 border-b py-2">
          {stagedAttachments.map((file, index) => (
            <Button
              type="button"
              variant="secondary"
              size="xs"
              className="max-w-56 font-normal"
              key={`${file.name}-${file.size}-${index}`}
              title="Remove attachment"
              onClick={() =>
                onStageAttachments?.(
                  stagedAttachments.filter((_, itemIndex) => itemIndex !== index),
                )
              }
            >
              <Paperclip /> <span className="truncate">{file.name}</span> ×
            </Button>
          ))}
        </div>
      )}
      {attachmentError && (
        <p role="alert" className="border-b py-2 text-xs text-destructive">
          {attachmentError}
        </p>
      )}
      {mode === "rich" ? (
        <EditorContent
          editor={editor}
          className={cn(
            "relative [&_.tiptap]:py-4 [&_.tiptap]:text-sm [&_.tiptap]:leading-[1.6] [&_.tiptap]:text-foreground/90 [&_.tiptap]:outline-none",
            compact ? "[&_.tiptap]:min-h-32 [&_.tiptap]:py-3" : "[&_.tiptap]:min-h-24",
            "[&_.tiptap_p]:my-1.5 [&_.tiptap_h1]:mt-6 [&_.tiptap_h1]:mb-2 [&_.tiptap_h1]:text-xl [&_.tiptap_h1]:font-semibold [&_.tiptap_h2]:mt-5 [&_.tiptap_h2]:mb-2 [&_.tiptap_h2]:text-lg [&_.tiptap_h2]:font-semibold [&_.tiptap_h3]:mt-4 [&_.tiptap_h3]:mb-1.5 [&_.tiptap_h3]:text-base [&_.tiptap_h3]:font-semibold",
            "[&_.tiptap_ul:not([data-type=taskList])]:my-2 [&_.tiptap_ul:not([data-type=taskList])]:list-disc [&_.tiptap_ul:not([data-type=taskList])]:pl-6 [&_.tiptap_ol]:my-2 [&_.tiptap_ol]:list-decimal [&_.tiptap_ol]:pl-6",
            "[&_.tiptap_ul[data-type=taskList]]:my-2 [&_.tiptap_ul[data-type=taskList]]:list-none [&_.tiptap_ul[data-type=taskList]]:pl-0 [&_.tiptap_ul[data-type=taskList]_li]:flex [&_.tiptap_ul[data-type=taskList]_li]:items-start [&_.tiptap_ul[data-type=taskList]_li]:gap-2 [&_.tiptap_ul[data-type=taskList]_li>label]:mt-1",
            "[&_.tiptap_blockquote]:my-3 [&_.tiptap_blockquote]:border-l-2 [&_.tiptap_blockquote]:pl-4 [&_.tiptap_blockquote]:text-muted-foreground",
            "[&_.tiptap_code]:rounded [&_.tiptap_code]:bg-muted [&_.tiptap_code]:px-1 [&_.tiptap_code]:font-mono [&_.tiptap_code]:text-[.85em] [&_.tiptap_pre]:my-3 [&_.tiptap_pre]:overflow-auto [&_.tiptap_pre]:rounded-lg [&_.tiptap_pre]:bg-muted [&_.tiptap_pre]:p-3 [&_.tiptap_pre_code]:bg-transparent [&_.tiptap_pre_code]:p-0",
            "[&_.tiptap_a]:cursor-pointer [&_.tiptap_a]:text-primary [&_.tiptap_a]:underline [&_.tiptap_hr]:my-5 [&_.tiptap_hr]:border-t",
            "[&_.ticket-attachment-image]:my-3 [&_.ticket-attachment-image]:max-h-[32rem] [&_.ticket-attachment-image]:max-w-full [&_.ticket-attachment-image]:rounded-lg [&_.ticket-attachment-image]:border [&_.ticket-attachment-image]:object-contain",
            "[&_.tiptap_p.is-editor-empty:first-child:before]:pointer-events-none [&_.tiptap_p.is-editor-empty:first-child:before]:float-left [&_.tiptap_p.is-editor-empty:first-child:before]:h-0 [&_.tiptap_p.is-editor-empty:first-child:before]:text-muted-foreground [&_.tiptap_p.is-editor-empty:first-child:before]:content-[attr(data-placeholder)]",
          )}
        />
      ) : (
        <Textarea
          aria-label="Raw Markdown"
          value={value}
          onChange={(event) => onChange(event.target.value)}
          className={cn(
            compact ? "min-h-32 py-3" : "min-h-48 py-4",
            "resize-y rounded-none border-0 px-0 font-mono text-[13px] leading-6 shadow-none dark:bg-transparent focus-visible:border-0 focus-visible:ring-0",
          )}
          onKeyDown={(event) => {
            if (onSave && (event.metaKey || event.ctrlKey) && event.key === "Enter") {
              event.preventDefault()
              void save()
            }
          }}
        />
      )}
      <EditorSlashMenu
        state={slash}
        menuId={slashMenuId}
        commands={filteredCommands}
        index={suggestionIndex}
        onIndex={setSuggestionIndex}
        onSelect={runSlashCommand}
      />
      <EditorSuggestionMenu
        state={editorSuggestions.state}
        menuId={suggestionMenuId}
        suggestions={editorSuggestions.suggestions}
        index={editorSuggestions.index}
        loading={editorSuggestions.loading}
        githubAvailable={editorSuggestions.githubAvailable}
        onIndex={editorSuggestions.setIndex}
        onSelect={(suggestion) => editor && editorSuggestions.select(editor, suggestion)}
      />
    </div>
  )
}

function FormatButton({
  label,
  active,
  onClick,
  children,
}: {
  label: string
  active?: boolean
  onClick: () => void
  children: React.ReactNode
}) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button
            variant={active ? "secondary" : "ghost"}
            size="icon-sm"
            aria-label={label}
            aria-pressed={active}
            onClick={onClick}
          />
        }
      >
        {children}
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  )
}
