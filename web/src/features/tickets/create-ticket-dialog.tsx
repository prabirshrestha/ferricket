import { useEffect, useState } from "react"
import { createTicket, updateTicket, uploadAttachment } from "../../api/tickets"
import { Button } from "../../components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "../../components/ui/dialog"
import { Input } from "../../components/ui/input"
import { Separator } from "../../components/ui/separator"
import type { Status, Ticket } from "../../types/ticket"
import { TicketPropertyToolbar } from "./create-ticket/ticket-property-toolbar"
import { MarkdownComposer } from "./markdown-editor"

type Props = {
  open: boolean
  parent?: Ticket
  initialStatus?: Status
  tickets: Ticket[]
  currentUser?: string
  onOpenChange: (value: boolean) => void
  onCreated: (ticket: Ticket) => void
}

export function CreateTicketDialog({
  open,
  parent,
  initialStatus = "open",
  tickets,
  currentUser,
  onOpenChange,
  onCreated,
}: Props) {
  const [title, setTitle] = useState("")
  const [description, setDescription] = useState("")
  const [type, setType] = useState("task")
  const [status, setStatus] = useState<Status>("open")
  const [priority, setPriority] = useState(2)
  const [assignee, setAssignee] = useState("")
  const [labels, setLabels] = useState<string[]>([])
  const [parentId, setParentId] = useState(parent?.id ?? "none")
  const [saving, setSaving] = useState(false)
  const [attachments, setAttachments] = useState<File[]>([])
  const [error, setError] = useState("")
  const [createdDraft, setCreatedDraft] = useState<Ticket>()
  const [uploadedMarkdown, setUploadedMarkdown] = useState<string[]>([])
  const assignees = [
    ...new Set(tickets.flatMap((ticket) => (ticket.assignee ? [ticket.assignee] : []))),
  ].sort()
  const parents = [
    { value: "none", label: "No parent" },
    ...tickets.map((ticket) => ({ value: ticket.id, label: ticket.title, detail: ticket.id })),
  ]

  useEffect(() => {
    if (open) {
      setParentId(parent?.id ?? "none")
      setStatus(initialStatus)
    }
  }, [open, parent, initialStatus])

  const reset = () => {
    setTitle("")
    setDescription("")
    setType("task")
    setStatus("open")
    setPriority(2)
    setAssignee("")
    setLabels([])
    setParentId("none")
    setAttachments([])
    setError("")
    setCreatedDraft(undefined)
    setUploadedMarkdown([])
  }
  const handleOpenChange = (value: boolean) => {
    if (!value) reset()
    onOpenChange(value)
  }
  const submit = async (event: React.FormEvent) => {
    event.preventDefault()
    if (!title.trim()) return
    setSaving(true)
    try {
      let ticket =
        createdDraft ??
        (await createTicket({
          title: title.trim(),
          description: description.trim(),
          priority,
          type,
          status,
          assignee: assignee.trim() || undefined,
          tags: labels,
          parent: parentId === "none" ? undefined : parentId,
        }))
      setCreatedDraft(ticket)
      const pendingMarkdown = [...uploadedMarkdown]
      for (const file of attachments) {
        const uploaded = await uploadAttachment(ticket.id, file)
        pendingMarkdown.push(uploaded.markdown)
        setUploadedMarkdown([...pendingMarkdown])
        setAttachments((current) => {
          const index = current.indexOf(file)
          return index < 0 ? current : current.filter((_, itemIndex) => itemIndex !== index)
        })
      }
      if (pendingMarkdown.length > 0) {
        const nextDescription = [ticket.description, ...pendingMarkdown]
          .filter(Boolean)
          .join("\n\n")
        ticket = await updateTicket(ticket.id, {
          revision: ticket.revision,
          description: nextDescription,
        })
        setCreatedDraft(ticket)
        setUploadedMarkdown([])
      }
      onCreated(ticket)
      handleOpenChange(false)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setSaving(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent
        className="top-[min(36vh,20rem)] w-[min(94vw,680px)] max-w-none gap-0 overflow-hidden p-0 sm:max-w-none"
        showCloseButton={false}
      >
        <form
          onSubmit={submit}
          onKeyDownCapture={(event) => {
            if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
              event.preventDefault()
              event.currentTarget.requestSubmit()
            }
          }}
        >
          <DialogHeader className="border-b px-4 py-2.5">
            <div className="flex items-center gap-2">
              <span className="grid size-5 place-items-center rounded bg-primary text-[10px] font-semibold text-primary-foreground">
                F
              </span>
              <DialogTitle className="text-xs font-medium">
                {parent ? "New sub-ticket" : "New ticket"}
              </DialogTitle>
            </div>
            <DialogDescription className="sr-only">
              Create a ticket in the current workspace.
            </DialogDescription>
          </DialogHeader>
          <div className="px-4 pt-3">
            <Input
              id="create-title"
              autoFocus
              className="h-auto rounded-none border-0 bg-transparent p-0 text-lg font-medium tracking-[-.015em] shadow-none dark:bg-transparent focus-visible:border-0 focus-visible:ring-0"
              value={title}
              onChange={(event) => setTitle(event.target.value)}
              placeholder="Issue title"
            />
            <MarkdownComposer
              editorKey={open ? "create-open" : "create-closed"}
              value={description}
              onChange={setDescription}
              tickets={tickets}
              currentUser={currentUser}
              stagedAttachments={attachments}
              onStageAttachments={setAttachments}
              onError={setError}
              compact
            />
            {error && (
              <p role="alert" className="pb-2 text-xs text-destructive">
                {error}
              </p>
            )}
          </div>
          <Separator />
          <div className="px-4 py-2.5">
            <TicketPropertyToolbar
              status={status}
              priority={priority}
              type={type}
              assignee={assignee}
              parentId={parentId}
              labels={labels}
              assignees={assignees}
              parents={parents}
              labelSuggestions={tickets.flatMap((ticket) => ticket.tags)}
              onStatusChange={setStatus}
              onPriorityChange={setPriority}
              onTypeChange={setType}
              onAssigneeChange={setAssignee}
              onParentChange={setParentId}
              onLabelsChange={setLabels}
            />
          </div>
          <Separator />
          <footer className="flex items-center gap-2 bg-muted/25 px-4 py-2">
            <span className="mr-auto hidden text-[11px] text-muted-foreground sm:block">
              ⌘ Enter to create
            </span>
            <Button type="button" variant="ghost" size="sm" onClick={() => handleOpenChange(false)}>
              Cancel
            </Button>
            <Button size="sm" disabled={!title.trim() || saving}>
              {saving ? "Creating…" : parent ? "Create sub-ticket" : "Create ticket"}
            </Button>
          </footer>
        </form>
      </DialogContent>
    </Dialog>
  )
}
