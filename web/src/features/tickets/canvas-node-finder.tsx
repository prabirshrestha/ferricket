import { Search } from "lucide-react"
import { useEffect, useMemo, useState } from "react"
import { Button } from "../../components/ui/button"
import {
  Command,
  CommandEmpty,
  CommandInput,
  CommandItem,
  CommandList,
  CommandShortcut,
  type CommandItemValue,
} from "../../components/ui/command"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "../../components/ui/dialog"
import { isInput } from "../../lib/format"
import { statusMeta, type Ticket } from "../../types/ticket"

type FinderEntry = CommandItemValue & { ticket: Ticket }

export function CanvasNodeFinder({
  tickets,
  onLocate,
}: {
  tickets: Ticket[]
  onLocate: (id: string) => void
}) {
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState("")
  const entries = useMemo<FinderEntry[]>(
    () =>
      tickets.map((ticket) => ({
        id: ticket.id,
        label: ticket.title,
        detail: ticket.id,
        keywords: [
          ticket.id,
          statusMeta[ticket.status].label,
          ticket.assignee,
          ticket.type,
          ...ticket.tags,
        ]
          .filter(Boolean)
          .join(" "),
        ticket,
      })),
    [tickets],
  )
  const filteredEntries = useMemo(
    () => entries.filter((entry) => canvasNodeMatches(entry.ticket, query)),
    [entries, query],
  )
  const changeOpen = (next: boolean) => {
    setOpen(next)
    if (!next) setQuery("")
  }

  useEffect(() => {
    const keydown = (event: KeyboardEvent) => {
      if (
        event.key === "/" &&
        !event.metaKey &&
        !event.ctrlKey &&
        !event.altKey &&
        !isInput(event.target)
      ) {
        event.preventDefault()
        changeOpen(true)
      }
    }
    window.addEventListener("keydown", keydown)
    return () => window.removeEventListener("keydown", keydown)
  }, [])

  const locate = (entry: FinderEntry) => {
    changeOpen(false)
    requestAnimationFrame(() => onLocate(entry.ticket.id))
  }

  return (
    <>
      <Button
        variant="outline"
        size="icon-sm"
        aria-label="Find ticket in dependency view"
        title="Find ticket (/)"
        onClick={() => changeOpen(true)}
      >
        <Search />
      </Button>
      <Dialog open={open} onOpenChange={changeOpen}>
        <DialogContent
          data-canvas-finder-open
          className="top-[16vh] z-[70] w-[min(92vw,620px)] max-w-none translate-y-0 gap-0 overflow-hidden p-0 sm:max-w-none"
          showCloseButton={false}
        >
          <DialogHeader className="sr-only">
            <DialogTitle>Find a ticket</DialogTitle>
            <DialogDescription>Search by ticket ID, title, label, or assignee.</DialogDescription>
          </DialogHeader>
          <Command
            items={entries}
            filteredItems={filteredEntries}
            filter={null}
            value={query}
            onValueChange={(value, details) => {
              if (details.reason !== "item-press") setQuery(value)
            }}
          >
            <CommandInput autoFocus placeholder="Find by ID or title…" aria-label="Find ticket" />
            <CommandList>
              <CommandEmpty>No matching tickets.</CommandEmpty>
              {filteredEntries.map((entry) => {
                const meta = statusMeta[entry.ticket.status]
                const StatusIcon = meta.icon
                return (
                  <CommandItem key={entry.id} value={entry} onClick={() => locate(entry)}>
                    <StatusIcon className="size-4 shrink-0" style={{ color: meta.color }} />
                    <span className="min-w-0 flex-1 truncate font-medium">{entry.label}</span>
                    <CommandShortcut className="font-mono normal-case tracking-normal">
                      {entry.detail}
                    </CommandShortcut>
                  </CommandItem>
                )
              })}
            </CommandList>
          </Command>
          <div className="flex gap-4 border-t bg-muted/30 px-4 py-2 text-[11px] text-muted-foreground">
            <span>
              <kbd>↑↓</kbd> Navigate
            </span>
            <span>
              <kbd>↵</kbd> Locate
            </span>
            <span>
              <kbd>Esc</kbd> Close
            </span>
          </div>
        </DialogContent>
      </Dialog>
    </>
  )
}

export function canvasNodeMatches(ticket: Ticket, query: string) {
  const normalized = query.trim().toLocaleLowerCase()
  if (!normalized) return true
  return [
    ticket.id,
    ticket.title,
    statusMeta[ticket.status].label,
    ticket.assignee,
    ticket.type,
    ...ticket.tags,
  ].some((part) => part?.toLocaleLowerCase().includes(normalized))
}
