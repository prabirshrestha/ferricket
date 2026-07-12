import { useEffect, useState } from "react"
import {
  ArrowLeft,
  ArrowRight,
  ArrowUpRight,
  Ban,
  Check,
  Copy,
  ExternalLink,
  FileText,
  Link2,
  ListTree,
  MoreHorizontal,
  Code2,
  Plus,
  X,
} from "lucide-react"
import { openTicketFile } from "../../api/tickets"
import { Button } from "../../components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "../../components/ui/dropdown-menu"
import { Input } from "../../components/ui/input"
import { ScrollArea } from "../../components/ui/scroll-area"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "../../components/ui/select"
import { Separator } from "../../components/ui/separator"
import { Tooltip, TooltipContent, TooltipTrigger } from "../../components/ui/tooltip"
import { SidebarTrigger } from "../../components/ui/sidebar"
import { formatDate } from "../../lib/format"
import { statusMeta, type Status, type Ticket } from "../../types/ticket"
import { MarkdownEditor } from "./markdown-editor"
import { LabelCombobox } from "./label-combobox"
import { SearchCombobox } from "./search-combobox"
import { ActivitySection } from "./activity-section"

type Props = {
  ticket: Ticket
  all: Ticket[]
  currentUser?: string
  error?: string
  onClose: () => void
  onUpdate: (id: string, body: Partial<Ticket>) => Promise<void>
  onRefresh: () => Promise<void>
  onCreateSubTicket: (ticket: Ticket) => void
  onSelect: (id: string) => void
  onBack: () => void
  onForward: () => void
  onError: (message: string) => void
  onRaw: () => void
}

export function TicketDetailView({
  ticket,
  all,
  currentUser,
  error,
  onClose,
  onUpdate,
  onRefresh,
  onCreateSubTicket,
  onSelect,
  onBack,
  onForward,
  onError,
  onRaw,
}: Props) {
  const [copied, setCopied] = useState(false)
  useEffect(() => {
    const navigate = (event: KeyboardEvent) => {
      if (
        event.target instanceof Element &&
        event.target.closest(
          "input, textarea, select, [contenteditable='true'], [role='textbox'], [role='combobox'], [role='listbox'], [role='menu']",
        )
      )
        return
      if (event.key === "ArrowLeft" || event.key === "h") {
        event.preventDefault()
        onBack()
      }
      if (event.key === "ArrowRight" || event.key === "l") {
        event.preventDefault()
        onForward()
      }
    }
    window.addEventListener("keydown", navigate)
    return () => window.removeEventListener("keydown", navigate)
  }, [onBack, onForward])
  const blockers = ticket.deps
    .map((id) => all.find((item) => item.id === id))
    .filter(Boolean) as Ticket[]
  const linked = ticket.links
    .map((id) => all.find((item) => item.id === id))
    .filter(Boolean) as Ticket[]
  const children = all.filter((item) => item.parent === ticket.id)
  const parent = all.find((item) => item.id === ticket.parent)
  const assignees = [
    ...new Set(all.flatMap((item) => (item.assignee ? [item.assignee] : []))),
  ].sort()
  const copyPath = async () => {
    await navigator.clipboard.writeText(ticket.path)
    setCopied(true)
    setTimeout(() => setCopied(false), 1200)
  }

  return (
    <div className="flex h-full min-h-0 flex-col bg-background">
      <header className="flex h-12 shrink-0 items-center gap-1 border-b px-3 sm:px-4">
        <SidebarTrigger className="mr-1" />
        <NavButton label="Browser back" onClick={onBack}>
          <ArrowLeft />
        </NavButton>
        <NavButton label="Browser forward" onClick={onForward}>
          <ArrowRight />
        </NavButton>
        <div className="ml-2 flex min-w-0 items-center gap-2 text-xs">
          <span className="shrink-0 font-mono font-medium text-muted-foreground">{ticket.id}</span>
          <span className="text-border">/</span>
          <span className="truncate font-medium text-foreground/80">{ticket.title}</span>
        </div>
        <div className="ml-auto flex shrink-0 items-center gap-1">
          <FileActions ticket={ticket} copied={copied} onCopy={copyPath} onRaw={onRaw} />
          <DropdownMenu>
            <DropdownMenuTrigger
              render={<Button variant="ghost" size="icon-sm" aria-label="Ticket actions" />}
            >
              <MoreHorizontal />
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-56">
              <DropdownMenuItem onClick={() => onCreateSubTicket(ticket)}>
                <Plus /> Create sub-ticket
              </DropdownMenuItem>
              <DropdownMenuItem
                onClick={() =>
                  void onUpdate(ticket.id, {
                    status: ticket.status === "closed" ? "open" : "closed",
                  })
                }
              >
                <Check /> {ticket.status === "closed" ? "Reopen ticket" : "Mark as done"}
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem onClick={() => void copyPath()}>
                <Copy /> Copy file path
              </DropdownMenuItem>
              <DropdownMenuItem onClick={onRaw}>
                <Code2 /> View raw file
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => void openTicketFile(ticket.id)}>
                <ExternalLink /> Open ticket file
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label="Close ticket details"
            onClick={onClose}
          >
            <X />
          </Button>
        </div>
      </header>
      {error && (
        <div className="flex shrink-0 items-center justify-between border-b border-destructive/30 bg-destructive/10 px-4 py-2 text-xs text-destructive">
          {error}
          <Button
            variant="ghost"
            size="icon-xs"
            aria-label="Dismiss error"
            onClick={() => onError("")}
          >
            <X />
          </Button>
        </div>
      )}
      <div className="grid min-h-0 flex-1 grid-cols-1 overflow-auto md:grid-cols-[minmax(0,1fr)_19rem] md:overflow-hidden xl:grid-cols-[minmax(0,1fr)_21rem]">
        <ScrollArea className="min-h-[55dvh] md:h-full md:min-h-0">
          <article className="mx-auto max-w-[52rem] px-6 py-8 sm:px-10 lg:px-12 lg:py-10">
            <Input
              className="h-auto rounded-none border-0 bg-transparent p-0 text-2xl font-semibold leading-tight tracking-[-.035em] shadow-none dark:bg-transparent focus-visible:border-0 focus-visible:ring-0 sm:text-[28px]"
              defaultValue={ticket.title}
              key={ticket.id + ticket.title}
              onBlur={(event) =>
                event.target.value !== ticket.title &&
                void onUpdate(ticket.id, { title: event.target.value })
              }
            />
            <MarkdownEditor
              key={ticket.id + ticket.revision}
              ticket={ticket}
              tickets={all}
              currentUser={currentUser}
              onSelect={onSelect}
              onSave={(description) => onUpdate(ticket.id, { description })}
              onError={onError}
            />
            <RelationshipSection
              title="Parent & sub-tickets"
              icon={ListTree}
              action={
                <Button variant="ghost" size="sm" onClick={() => onCreateSubTicket(ticket)}>
                  <Plus /> Add sub-ticket
                </Button>
              }
              items={[
                ...(parent ? [{ item: parent, label: "Parent" }] : []),
                ...children.map((item) => ({ item, label: "" })),
              ]}
              onSelect={onSelect}
              emptyAction={!parent && children.length === 0}
            />
            {blockers.length > 0 && (
              <RelationshipSection
                title="Blocked by"
                icon={Ban}
                items={blockers.map((item) => ({ item, label: "Blocker" }))}
                onSelect={onSelect}
              />
            )}
            {linked.length > 0 && (
              <RelationshipSection
                title="Related tickets"
                icon={Link2}
                items={linked.map((item) => ({ item, label: "Related" }))}
                onSelect={onSelect}
              />
            )}
            <ActivitySection ticket={ticket} onRefresh={onRefresh} onError={onError} />
          </article>
        </ScrollArea>
        <aside className="min-w-0 border-t bg-background px-5 py-6 md:overflow-y-auto md:border-t-0 md:border-l">
          <h3 className="mb-3 text-xs font-semibold">Properties</h3>
          <Property label="Status">
            <Select
              value={ticket.status}
              onValueChange={(status) =>
                status && void onUpdate(ticket.id, { status: status as Status })
              }
            >
              <SelectTrigger className="w-full">
                <SelectValue>{statusMeta[ticket.status].label}</SelectValue>
              </SelectTrigger>
              <SelectContent>
                {(Object.keys(statusMeta) as Status[]).map((status) => (
                  <SelectItem key={status} value={status}>
                    {statusMeta[status].label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Property>
          <Property label="Priority">
            <Select
              value={String(ticket.priority)}
              onValueChange={(priority) =>
                priority !== null && void onUpdate(ticket.id, { priority: Number(priority) })
              }
            >
              <SelectTrigger className="w-full">
                <SelectValue>P{ticket.priority}</SelectValue>
              </SelectTrigger>
              <SelectContent>
                {[0, 1, 2, 3, 4].map((value) => (
                  <SelectItem key={value} value={String(value)}>
                    P{value}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Property>
          <Property label="Assignee">
            <SearchCombobox
              contentClassName="max-w-[min(22rem,var(--available-width))]"
              value={ticket.assignee ?? ""}
              options={[
                { value: "", label: "Unassigned" },
                ...assignees.map((value) => ({ value, label: value })),
              ]}
              placeholder="Search assignees…"
              onChange={(assignee) => void onUpdate(ticket.id, { assignee })}
            />
          </Property>
          <Property label="Labels">
            <LabelCombobox
              contentClassName="max-w-[min(22rem,var(--available-width))]"
              value={ticket.tags}
              options={all.flatMap((item) => item.tags)}
              onChange={(tags) => void onUpdate(ticket.id, { tags })}
            />
          </Property>
          <Property label="Parent">
            <div className="flex min-w-0 items-center gap-1">
              <div className="min-w-0 flex-1">
                <SearchCombobox
                  contentClassName="w-[min(22rem,var(--available-width))] max-w-[calc(100vw-2rem)]"
                  value={ticket.parent ?? "none"}
                  options={[
                    { value: "none", label: "No parent" },
                    ...all
                      .filter((item) => item.id !== ticket.id)
                      .map((item) => ({ value: item.id, label: item.title, detail: item.id })),
                  ]}
                  placeholder="Search tickets…"
                  onChange={(parent) =>
                    void onUpdate(ticket.id, { parent: parent === "none" ? "" : parent })
                  }
                />
              </div>
              {parent && (
                <Tooltip>
                  <TooltipTrigger
                    render={
                      <Button
                        variant="ghost"
                        size="icon-sm"
                        aria-label={"Open parent " + parent.id}
                        onClick={() => onSelect(parent.id)}
                      />
                    }
                  >
                    <ArrowUpRight />
                  </TooltipTrigger>
                  <TooltipContent>Open parent · {parent.id}</TooltipContent>
                </Tooltip>
              )}
            </div>
          </Property>
          <Property label="Created">
            <span>{formatDate(ticket.created)}</span>
          </Property>
          <Separator className="my-5" />
          <h3 className="mb-3 text-xs font-semibold">File</h3>
          <Button
            className="h-auto w-full justify-start overflow-hidden px-3 py-2.5 text-left text-xs font-normal text-muted-foreground"
            variant="outline"
            onClick={() => void copyPath()}
            title={ticket.path}
          >
            <FileText className="size-4 shrink-0" />
            <span className="min-w-0 flex-1 truncate font-mono">{ticket.path}</span>
            {copied ? <Check className="size-4 text-green-500" /> : <Copy className="size-4" />}
          </Button>
          <Button className="mt-2 w-full" variant="outline" size="sm" onClick={onRaw}>
            <Code2 /> View raw file
          </Button>
          <Button
            className="mt-2 w-full"
            variant="outline"
            size="sm"
            onClick={() => void openTicketFile(ticket.id)}
          >
            <ExternalLink /> Open in default editor
          </Button>
        </aside>
      </div>
    </div>
  )
}

function RelationshipSection({
  title,
  icon: Icon,
  items,
  action,
  onSelect,
  emptyAction = false,
}: {
  title: string
  icon: typeof ListTree
  items: Array<{ item: Ticket; label: string }>
  action?: React.ReactNode
  onSelect: (id: string) => void
  emptyAction?: boolean
}) {
  if (items.length === 0 && !emptyAction) return null
  return (
    <section className="mt-10">
      <div className="mb-3 flex items-center">
        <h3 className="flex items-center gap-2 text-xs font-semibold text-foreground">
          <Icon className="size-4 text-muted-foreground" />
          {title}
        </h3>
        <div className="ml-auto">{action}</div>
      </div>
      {items.length > 0 ? (
        <div className="overflow-hidden rounded-lg border">
          {items.map(({ item, label }) => (
            <Button
              key={item.id}
              variant="ghost"
              className="group h-auto w-full justify-start gap-3 rounded-none border-b px-3 py-3 text-left text-sm font-normal last:border-b-0"
              onClick={() => onSelect(item.id)}
            >
              {label && (
                <span className="w-16 shrink-0 whitespace-nowrap text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
                  {label}
                </span>
              )}
              <span className="shrink-0 font-mono text-xs text-muted-foreground">{item.id}</span>
              <span className="min-w-0 flex-1 truncate font-medium">{item.title}</span>
              <ArrowRight className="size-4 shrink-0 text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100" />
            </Button>
          ))}
        </div>
      ) : (
        <p className="rounded-lg border border-dashed px-4 py-5 text-sm text-muted-foreground">
          No sub-tickets yet.
        </p>
      )}
    </section>
  )
}

function Property({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="grid min-h-11 grid-cols-[78px_minmax(0,1fr)] items-center gap-2 text-xs">
      <span className="text-muted-foreground">{label}</span>
      <div className="min-w-0 text-foreground/90">{children}</div>
    </div>
  )
}
function NavButton({
  label,
  onClick,
  children,
}: {
  label: string
  onClick: () => void
  children: React.ReactNode
}) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={<Button variant="ghost" size="icon-sm" aria-label={label} onClick={onClick} />}
      >
        {children}
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  )
}
function FileActions({
  ticket,
  copied,
  onCopy,
  onRaw,
}: {
  ticket: Ticket
  copied: boolean
  onCopy: () => void
  onRaw: () => void
}) {
  return (
    <div className="hidden items-center gap-1 sm:flex">
      <Tooltip>
        <TooltipTrigger
          render={<Button variant="ghost" size="icon-sm" onClick={() => void onCopy()} />}
        >
          {copied ? <Check /> : <Copy />}
        </TooltipTrigger>
        <TooltipContent>Copy file path</TooltipContent>
      </Tooltip>
      <Tooltip>
        <TooltipTrigger render={<Button variant="ghost" size="icon-sm" onClick={onRaw} />}>
          <Code2 />
        </TooltipTrigger>
        <TooltipContent>View raw file</TooltipContent>
      </Tooltip>
      <Tooltip>
        <TooltipTrigger
          render={
            <Button variant="ghost" size="icon-sm" onClick={() => void openTicketFile(ticket.id)} />
          }
        >
          <ExternalLink />
        </TooltipTrigger>
        <TooltipContent>Open ticket file</TooltipContent>
      </Tooltip>
    </div>
  )
}
