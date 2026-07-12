import { useMemo, useState } from "react"
import { Archive, Ban, CirclePlus, Inbox, LayoutList, PanelLeft, Radio, Search } from "lucide-react"
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "../ui/dialog"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandGroupLabel,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
  CommandShortcut,
} from "../ui/command"
import { statusMeta, type Ticket, type View } from "../../types/ticket"

type Props = {
  open: boolean
  tickets: Ticket[]
  onOpenChange: (open: boolean) => void
  onTicket: (id: string) => void
  onCreate: () => void
  onView: (view: View) => void
  onToggleSidebar: () => void
  onSaveView: () => void
  onToggleLive: () => void
}

type Entry = {
  id: string
  label: string
  keywords: string
  detail: string
  icon: typeof Search
  action: () => void
  group: "Actions" | "Tickets"
}

export type CommandPaletteSearchEntry = Pick<Entry, "label" | "keywords" | "detail" | "group">

export function filterCommandPaletteEntries<T extends CommandPaletteSearchEntry>(
  entries: readonly T[],
  query: string,
): T[] {
  const normalized = query.trim().toLocaleLowerCase()
  if (!normalized) return [...entries]
  const matches = entries.filter((entry) =>
    `${entry.label} ${entry.keywords} ${entry.detail}`.toLocaleLowerCase().includes(normalized),
  )
  return [
    ...matches.filter((entry) => entry.group === "Tickets"),
    ...matches.filter((entry) => entry.group === "Actions"),
  ]
}

export function CommandPalette({
  open,
  tickets,
  onOpenChange,
  onTicket,
  onCreate,
  onView,
  onToggleSidebar,
  onSaveView,
  onToggleLive,
}: Props) {
  const [query, setQuery] = useState("")
  const entries = useMemo<Entry[]>(
    () => [
      {
        id: "create",
        label: "Create ticket",
        detail: "C",
        keywords: "new add create",
        icon: CirclePlus,
        action: onCreate,
        group: "Actions",
      },
      {
        id: "active",
        label: "Go to active tickets",
        detail: "View",
        keywords: "active inbox",
        icon: Inbox,
        action: () => onView("active"),
        group: "Actions",
      },
      {
        id: "all",
        label: "Go to all tickets",
        detail: "View",
        keywords: "all tickets",
        icon: LayoutList,
        action: () => onView("all"),
        group: "Actions",
      },
      {
        id: "blocked",
        label: "Go to blocked tickets",
        detail: "View",
        keywords: "blocked dependency",
        icon: Ban,
        action: () => onView("blocked"),
        group: "Actions",
      },
      {
        id: "closed",
        label: "Go to closed tickets",
        detail: "View",
        keywords: "closed done archive",
        icon: Archive,
        action: () => onView("closed"),
        group: "Actions",
      },
      {
        id: "sidebar",
        label: "Toggle sidebar",
        detail: "Action",
        keywords: "sidebar navigation collapse",
        icon: PanelLeft,
        action: onToggleSidebar,
        group: "Actions",
      },
      {
        id: "save-view",
        label: "Save current view",
        detail: "Action",
        keywords: "save custom view filters display",
        icon: Archive,
        action: onSaveView,
        group: "Actions",
      },
      {
        id: "toggle-live",
        label: "Pause or resume live updates",
        detail: "Action",
        keywords: "live pause resume filesystem watch",
        icon: Radio,
        action: onToggleLive,
        group: "Actions",
      },
      ...tickets.map((ticket) => ({
        id: "ticket:" + ticket.id,
        label: ticket.title,
        detail: ticket.id,
        keywords: [
          ticket.id,
          ticket.title,
          ticket.description,
          ticket.assignee,
          ticket.type,
          ...ticket.tags,
        ]
          .filter(Boolean)
          .join(" "),
        icon: statusMeta[ticket.status].icon,
        action: () => onTicket(ticket.id),
        group: "Tickets" as const,
      })),
    ],
    [tickets, onCreate, onView, onTicket, onToggleSidebar, onSaveView, onToggleLive],
  )
  const filteredEntries = useMemo(
    () => filterCommandPaletteEntries(entries, query),
    [entries, query],
  )
  const groups = (
    query.trim() ? (["Tickets", "Actions"] as const) : (["Actions", "Tickets"] as const)
  )
    .map((group) => ({
      group,
      entries: filteredEntries.filter((entry) => entry.group === group),
    }))
    .filter(({ entries }) => entries.length > 0)
  const changeOpen = (next: boolean) => {
    onOpenChange(next)
    if (!next) setQuery("")
  }
  const choose = (entry: Entry) => {
    changeOpen(false)
    requestAnimationFrame(entry.action)
  }

  return (
    <Dialog open={open} onOpenChange={changeOpen}>
      <DialogContent
        className="top-[18vh] w-[min(92vw,640px)] max-w-none translate-y-0 gap-0 overflow-hidden p-0 sm:max-w-none"
        showCloseButton={false}
      >
        <DialogHeader className="sr-only">
          <DialogTitle>Command palette</DialogTitle>
          <DialogDescription>Search tickets, views, and actions.</DialogDescription>
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
          <CommandInput
            autoFocus
            placeholder="Search tickets, views, and actions…"
            aria-label="Command search"
          />
          <CommandList>
            <CommandEmpty>No matching tickets or actions.</CommandEmpty>
            {groups.map(({ group, entries: groupEntries }, index) => (
              <div key={group}>
                {index > 0 && <CommandSeparator />}
                <CommandGroup>
                  <CommandGroupLabel>{group}</CommandGroupLabel>
                  {groupEntries.map((entry) => {
                    const Icon = entry.icon
                    return (
                      <CommandItem key={entry.id} value={entry} onClick={() => choose(entry)}>
                        <Icon className="size-4 shrink-0 text-muted-foreground" />
                        <span className="min-w-0 flex-1 truncate font-medium">{entry.label}</span>
                        <CommandShortcut>{entry.detail}</CommandShortcut>
                      </CommandItem>
                    )
                  })}
                </CommandGroup>
              </div>
            ))}
          </CommandList>
        </Command>
        <div className="flex gap-4 border-t bg-muted/30 px-4 py-2 text-[11px] text-muted-foreground">
          <span>
            <kbd>↑↓</kbd> Navigate
          </span>
          <span>
            <kbd>↵</kbd> Open
          </span>
          <span>
            <kbd>Esc</kbd> Close
          </span>
        </div>
      </DialogContent>
    </Dialog>
  )
}
