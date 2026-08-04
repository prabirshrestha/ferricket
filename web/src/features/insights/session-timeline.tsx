import { CheckCircle2, CircleStop, Clock3, MessageSquare, Send, Waypoints } from "lucide-react"
import type { SessionEvent, SessionEventKind } from "../../types/session"

const kindMeta: Record<
  SessionEventKind,
  { label: string; icon: typeof MessageSquare; className: string }
> = {
  prompt: {
    label: "Prompt",
    icon: MessageSquare,
    className: "bg-blue-500/10 text-blue-700 dark:text-blue-300",
  },
  queued: {
    label: "Queued",
    icon: Clock3,
    className: "bg-amber-500/10 text-amber-700 dark:text-amber-300",
  },
  steering: {
    label: "Steering",
    icon: Waypoints,
    className: "bg-violet-500/10 text-violet-700 dark:text-violet-300",
  },
  completed: {
    label: "Completed",
    icon: CheckCircle2,
    className: "bg-emerald-500/10 text-emerald-700 dark:text-emerald-300",
  },
  stopped: {
    label: "Stopped",
    icon: CircleStop,
    className: "bg-red-500/10 text-red-700 dark:text-red-300",
  },
  ended: {
    label: "Ended",
    icon: Send,
    className: "bg-muted text-muted-foreground",
  },
}

export function sessionEventsInWindow(events: SessionEvent[], start: Date, end: Date) {
  return events
    .filter((event) => {
      const timestamp = new Date(event.timestamp)
      return !Number.isNaN(timestamp.getTime()) && timestamp >= start && timestamp <= end
    })
    .sort((left, right) => right.timestamp.localeCompare(left.timestamp))
}

export function SessionTimeline({
  events,
  warnings,
  error,
}: {
  events: SessionEvent[]
  warnings: string[]
  error: string
}) {
  const sessions = new Set(events.map((event) => event.session_id)).size
  return (
    <section className="overflow-hidden rounded-xl border bg-card">
      <header className="border-b px-4 py-3">
        <h2 className="text-sm font-semibold">Agent session timeline</h2>
        <p className="mt-0.5 text-xs text-muted-foreground">
          {events.length} {events.length === 1 ? "event" : "events"} across {sessions}{" "}
          {sessions === 1 ? "session" : "sessions"} · newest first
        </p>
      </header>
      {(error || warnings.length > 0) && (
        <div className="grid gap-1 border-b bg-amber-500/5 px-4 py-2 text-xs text-amber-800 dark:text-amber-200">
          {error && <span>{error}</span>}
          {warnings.map((warning, index) => (
            <span key={`${index}:${warning}`}>{warning}</span>
          ))}
        </div>
      )}
      {events.length ? (
        <ol className="divide-y">
          {events.map((event) => (
            <SessionTimelineItem key={`${event.session_id}:${event.id}`} event={event} />
          ))}
        </ol>
      ) : (
        <div className="grid min-h-36 place-items-center px-4 py-8 text-center text-sm text-muted-foreground">
          <div className="grid max-w-md justify-items-center gap-2">
            <MessageSquare className="size-5" />
            <p>No agent prompts were recorded in this range.</p>
            <p className="text-xs">
              Run fer from a GitHub Copilot CLI session to link its human prompts to this workspace.
            </p>
          </div>
        </div>
      )}
    </section>
  )
}

function SessionTimelineItem({ event }: { event: SessionEvent }) {
  const meta = kindMeta[event.kind]
  const Icon = meta.icon
  return (
    <li className="grid grid-cols-[auto_minmax(0,1fr)] gap-3 px-4 py-3">
      <div className={`mt-0.5 grid size-7 place-items-center rounded-full ${meta.className}`}>
        <Icon className="size-3.5" />
      </div>
      <div className="min-w-0">
        <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
          <span className="text-xs font-medium">{meta.label}</span>
          <time className="text-[11px] text-muted-foreground" dateTime={event.timestamp}>
            {formatTimestamp(event.timestamp)}
          </time>
          <span className="font-mono text-[10px] text-muted-foreground">
            {event.session_id.slice(0, 8)}
          </span>
        </div>
        {event.content && <EventContent content={event.content} />}
      </div>
    </li>
  )
}

function EventContent({ content }: { content: string }) {
  const compact = content.replace(/\s+/g, " ").trim()
  if (content.includes("\n") || compact.length > 180) {
    return (
      <details className="mt-1.5 text-xs">
        <summary className="cursor-pointer text-foreground">{truncate(compact, 180)}</summary>
        <p className="mt-2 whitespace-pre-wrap break-words rounded-md bg-muted/50 p-2.5 text-muted-foreground">
          {content}
        </p>
      </details>
    )
  }
  return <p className="mt-1.5 whitespace-pre-wrap break-words text-xs">{content}</p>
}

function formatTimestamp(value: string) {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(date)
}

function truncate(value: string, size: number) {
  return value.length > size ? value.slice(0, size - 1) + "…" : value
}
