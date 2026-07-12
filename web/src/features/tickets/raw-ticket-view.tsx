import { useState } from "react"
import { ArrowLeft, Check, Copy, ExternalLink, FileCode2, X } from "lucide-react"
import { openTicketFile } from "../../api/tickets"
import { SidebarTrigger } from "../../components/ui/sidebar"
import { Button } from "../../components/ui/button"
import { ScrollArea } from "../../components/ui/scroll-area"
import type { Ticket } from "../../types/ticket"

type Props = {
  ticket: Ticket
  onTicket: () => void
  onClose: () => void
  error?: string
  onError: (message: string) => void
}

export function RawTicketView({ ticket, onTicket, onClose, error, onError }: Props) {
  const [copied, setCopied] = useState<"path" | "content" | null>(null)
  const copy = async (kind: "path" | "content", value: string) => {
    try {
      await navigator.clipboard.writeText(value)
      setCopied(kind)
      setTimeout(() => setCopied(null), 1200)
    } catch (reason) {
      onError(`Could not copy ${kind}: ${String(reason)}`)
    }
  }

  return (
    <div className="flex h-full min-h-0 flex-col bg-background">
      <header className="flex h-12 shrink-0 items-center gap-1 border-b px-3 sm:px-4">
        <SidebarTrigger className="mr-1" />
        <Button variant="ghost" size="sm" onClick={onTicket}>
          <ArrowLeft /> Ticket
        </Button>
        <div className="ml-2 flex min-w-0 items-center gap-2 text-xs">
          <FileCode2 className="size-4 shrink-0 text-muted-foreground" />
          <span className="shrink-0 font-mono font-medium text-muted-foreground">{ticket.id}</span>
          <span className="text-border">/</span>
          <span className="truncate font-medium text-foreground/80">Raw ticket file</span>
        </div>
        <div className="ml-auto flex items-center gap-1">
          <Button variant="ghost" size="sm" onClick={() => void copy("path", ticket.path)}>
            {copied === "path" ? <Check /> : <Copy />} Copy path
          </Button>
          <Button variant="ghost" size="sm" onClick={() => void openTicketFile(ticket.id)}>
            <ExternalLink /> Open in editor
          </Button>
          <Button variant="ghost" size="icon-sm" aria-label="Close raw file" onClick={onClose}>
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
      <div className="flex min-h-0 flex-1 flex-col">
        <div className="flex shrink-0 items-center gap-3 border-b bg-muted/20 px-5 py-3">
          <div className="min-w-0 flex-1">
            <h1 className="truncate text-sm font-medium">{ticket.title}</h1>
            <p className="mt-0.5 truncate font-mono text-[11px] text-muted-foreground">
              {ticket.path}
            </p>
          </div>
          <Button variant="outline" size="sm" onClick={() => void copy("content", ticket.raw)}>
            {copied === "content" ? <Check /> : <Copy />} Copy contents
          </Button>
        </div>
        <ScrollArea className="min-h-0 flex-1">
          <pre className="min-h-full overflow-x-auto p-5 font-mono text-xs leading-6 text-foreground/90 selection:bg-primary/20 sm:p-7">
            <code>{ticket.raw}</code>
          </pre>
        </ScrollArea>
      </div>
    </div>
  )
}
