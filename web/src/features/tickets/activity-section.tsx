import { useState } from "react"
import { Check, MoreHorizontal, Pencil, Send, Trash2, X } from "lucide-react"
import { addNote, deleteNote, editNote } from "../../api/tickets"
import { Button } from "../../components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../../components/ui/dialog"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "../../components/ui/dropdown-menu"
import { Input } from "../../components/ui/input"
import { Textarea } from "../../components/ui/textarea"
import { formatDateTime, initials } from "../../lib/format"
import type { Ticket } from "../../types/ticket"

type Props = {
  ticket: Ticket
  onRefresh: () => Promise<void>
  onError: (message: string) => void
}

export function ActivitySection({ ticket, onRefresh, onError }: Props) {
  const [note, setNote] = useState("")
  const [editing, setEditing] = useState<number | null>(null)
  const [draft, setDraft] = useState("")
  const [deleting, setDeleting] = useState<number | null>(null)
  const [saving, setSaving] = useState(false)

  const mutate = async (action: () => Promise<Ticket>) => {
    setSaving(true)
    try {
      await action()
      await onRefresh()
      return true
    } catch (reason) {
      onError(String(reason))
      await onRefresh()
      setEditing(null)
      setDeleting(null)
      return false
    } finally {
      setSaving(false)
    }
  }
  const submit = async () => {
    if (!note.trim()) return
    if (await mutate(() => addNote(ticket.id, ticket.revision, note.trim()))) setNote("")
  }
  const saveEdit = async () => {
    if (editing === null || !draft.trim()) return
    if (await mutate(() => editNote(ticket.id, ticket.revision, editing, draft.trim()))) {
      setEditing(null)
      setDraft("")
    }
  }
  const confirmDelete = async () => {
    if (deleting === null) return
    if (await mutate(() => deleteNote(ticket.id, ticket.revision, deleting))) setDeleting(null)
  }

  return (
    <section className="mt-10">
      <h3 className="mb-4 text-xs font-semibold text-foreground">Activity</h3>
      {ticket.notes.length > 0 && (
        <div className="mb-4 space-y-4">
          {ticket.notes.map((item, index) => (
            <div className="group flex gap-3" key={(item.timestamp ?? "note") + index}>
              <span className="grid size-7 shrink-0 place-items-center rounded-full border bg-muted text-[10px] font-medium text-muted-foreground">
                {initials(ticket.assignee)}
              </span>
              <div className="min-w-0 flex-1 rounded-lg border bg-card px-3 py-2.5">
                <div className="mb-1 flex min-h-6 items-center text-xs text-muted-foreground">
                  <span>{ticket.assignee || "Contributor"}</span>
                  {item.timestamp && <span className="ml-2">{formatDateTime(item.timestamp)}</span>}
                  <DropdownMenu>
                    <DropdownMenuTrigger
                      render={
                        <Button
                          variant="ghost"
                          size="icon-xs"
                          className="ml-auto sm:opacity-0 sm:group-hover:opacity-100 sm:focus-visible:opacity-100 sm:data-popup-open:opacity-100"
                          aria-label="Note actions"
                        />
                      }
                    >
                      <MoreHorizontal />
                    </DropdownMenuTrigger>
                    <DropdownMenuContent align="end">
                      <DropdownMenuItem
                        onClick={() => {
                          setEditing(index)
                          setDraft(item.text)
                        }}
                      >
                        <Pencil /> Edit note
                      </DropdownMenuItem>
                      <DropdownMenuItem variant="destructive" onClick={() => setDeleting(index)}>
                        <Trash2 /> Delete note
                      </DropdownMenuItem>
                    </DropdownMenuContent>
                  </DropdownMenu>
                </div>
                {editing === index ? (
                  <div className="space-y-2">
                    <Textarea
                      autoFocus
                      className="min-h-24 resize-y"
                      value={draft}
                      onChange={(event) => setDraft(event.target.value)}
                      onKeyDown={(event) => {
                        if ((event.metaKey || event.ctrlKey) && event.key === "Enter")
                          void saveEdit()
                        if (event.key === "Escape") setEditing(null)
                      }}
                    />
                    <div className="flex justify-end gap-1.5">
                      <Button variant="ghost" size="sm" onClick={() => setEditing(null)}>
                        <X /> Cancel
                      </Button>
                      <Button size="sm" disabled={saving || !draft.trim()} onClick={saveEdit}>
                        <Check /> Save
                      </Button>
                    </div>
                  </div>
                ) : (
                  <p className="whitespace-pre-wrap text-sm leading-6">{item.text}</p>
                )}
              </div>
            </div>
          ))}
        </div>
      )}
      <div className="flex items-center gap-2 rounded-lg border bg-card p-2.5">
        <span className="grid size-7 shrink-0 place-items-center rounded-full border bg-muted text-[10px] font-medium text-muted-foreground">
          {initials(ticket.assignee)}
        </span>
        <Input
          className="h-8 flex-1 border-0 bg-transparent text-sm shadow-none dark:bg-transparent focus-visible:border-0 focus-visible:ring-0"
          value={note}
          onChange={(event) => setNote(event.target.value)}
          placeholder="Leave a note…"
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault()
              void submit()
            }
          }}
        />
        <Button size="icon-sm" variant="ghost" disabled={saving || !note.trim()} onClick={submit}>
          <Send />
          <span className="sr-only">Add note</span>
        </Button>
      </div>
      <Dialog open={deleting !== null} onOpenChange={(open) => !open && setDeleting(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete this note?</DialogTitle>
            <DialogDescription>This removes the note from the ticket file.</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDeleting(null)}>
              Cancel
            </Button>
            <Button variant="destructive" disabled={saving} onClick={confirmDelete}>
              <Trash2 /> Delete note
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </section>
  )
}
