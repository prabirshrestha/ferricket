import {
  AlertTriangle,
  ArrowRight,
  ChartNoAxesGantt,
  GitBranch,
  GitFork,
  Unplug,
} from "lucide-react"
import { useMemo, useRef, useState } from "react"
import { Badge } from "../../components/ui/badge"
import { Checkbox } from "../../components/ui/checkbox"
import { Toggle } from "../../components/ui/toggle"
import { CanvasFocusToggle } from "../../components/layout/canvas-focus-toggle"
import type { Density } from "../../hooks/use-display-preferences"
import { initials } from "../../lib/format"
import { cn } from "../../lib/utils"
import { statusMeta, type Status, type Ticket } from "../../types/ticket"
import { Priority } from "./priority"
import { buildDependencyRoadmap } from "./dependency-roadmap"
import { CanvasNodeFinder } from "./canvas-node-finder"
import { InProgressCardPulse } from "./in-progress-card-pulse"

const columnWidth = 268
const columnGap = 56
const cardHeight = 112
const cardGap = 12
const headerHeight = 54

type Props = {
  tickets: Ticket[]
  contextIds?: Set<string>
  density: Density
  selectedIds: Set<string>
  canvasFocused: boolean
  onCanvasFocusedChange: (focused: boolean) => void
  onToggle: (id: string, range: boolean) => void
  onSelect: (id: string) => void
}

export function DependencyRoadmapView({
  tickets,
  contextIds = new Set(),
  density,
  selectedIds,
  canvasFocused,
  onCanvasFocusedChange,
  onToggle,
  onSelect,
}: Props) {
  const [focusedTicket, setFocusedTicket] = useState<string | null>(null)
  const [locatedTicket, setLocatedTicket] = useState<string | null>(null)
  const [showAllConnections, setShowAllConnections] = useState(false)
  const scrollContainer = useRef<HTMLDivElement | null>(null)
  const roadmap = useMemo(() => buildDependencyRoadmap(tickets, contextIds), [contextIds, tickets])
  const tracedTicket = focusedTicket ?? locatedTicket
  const visibleEdges = useMemo(
    () =>
      tracedTicket
        ? roadmap.edges.filter((edge) => edge.from === tracedTicket || edge.to === tracedTicket)
        : showAllConnections
          ? roadmap.edges
          : [],
    [roadmap.edges, showAllConnections, tracedTicket],
  )
  const statusCounts = useMemo(
    () =>
      Object.fromEntries(
        (["open", "in_progress", "closed"] as const).map((status) => [
          status,
          tickets.filter((ticket) => ticket.status === status).length,
        ]),
      ),
    [tickets],
  )
  const positions = useMemo(
    () =>
      new Map(
        roadmap.stages.flatMap((stage) =>
          stage.tickets.map((ticket, row) => [
            ticket.id,
            {
              left: stage.index * (columnWidth + columnGap),
              top: headerHeight + row * (cardHeight + cardGap),
              stage: stage.index,
            },
          ]),
        ),
      ),
    [roadmap.stages],
  )
  const maxRows = Math.max(1, ...roadmap.stages.map((stage) => stage.tickets.length))
  const width = Math.max(
    columnWidth,
    roadmap.stages.length * columnWidth + Math.max(0, roadmap.stages.length - 1) * columnGap,
  )
  const height = headerHeight + maxRows * (cardHeight + cardGap)

  return (
    <section
      data-canvas-focused={canvasFocused || undefined}
      className={cn(
        "flex h-full min-h-0 flex-col overflow-hidden bg-background",
        canvasFocused && "fixed inset-0 z-50 h-dvh w-dvw",
      )}
    >
      <header
        className={cn(
          "flex shrink-0 flex-wrap items-center gap-x-4 gap-y-1 border-b px-5 py-2.5",
          canvasFocused && "px-3 py-1.5",
        )}
      >
        <div className="flex items-center gap-2 text-xs font-medium">
          <ChartNoAxesGantt className="size-4 text-muted-foreground" /> Dependency roadmap
        </div>
        <p className="text-[11px] text-muted-foreground">
          Each stage unlocks after the stage to its left · Tickets in one stage can run in parallel
        </p>
        <div className="ml-auto flex flex-wrap items-center justify-end gap-1.5 text-[11px] text-muted-foreground">
          {(["open", "in_progress", "closed"] as const).map((status) => {
            const meta = statusMeta[status]
            const palette = roadmapStatusPalette(status)
            const StatusIcon = meta.icon
            return (
              <span
                key={status}
                className="inline-flex h-7 items-center gap-1.5 rounded-md border bg-card px-2"
                style={{
                  borderColor: `color-mix(in srgb, ${palette.color} 32%, var(--border))`,
                  backgroundColor: `color-mix(in srgb, ${palette.color} 7%, var(--card))`,
                }}
              >
                <StatusIcon className="size-3.5" color={palette.color} />
                {meta.label}{" "}
                <strong className="font-medium text-foreground">{statusCounts[status]}</strong>
              </span>
            )
          })}
          <Toggle
            size="sm"
            variant="outline"
            pressed={showAllConnections}
            onPressedChange={setShowAllConnections}
            aria-label="Show all dependency connections"
            className="ml-1 text-[11px]"
          >
            <GitFork /> All connections
          </Toggle>
          <CanvasNodeFinder
            tickets={tickets}
            onLocate={(id) => {
              setLocatedTicket(id)
              requestAnimationFrame(() => {
                const card = scrollContainer.current?.querySelector<HTMLElement>(
                  `[data-ticket-id="${CSS.escape(id)}"]`,
                )
                card?.scrollIntoView({ behavior: "smooth", block: "center", inline: "center" })
                card?.focus({ preventScroll: true })
              })
            }}
          />
          <CanvasFocusToggle focused={canvasFocused} onFocusedChange={onCanvasFocusedChange} />
        </div>
      </header>
      <div className="shrink-0 border-b bg-muted/20 px-5 py-1.5 text-[11px] text-muted-foreground">
        {showAllConnections
          ? "Showing all " +
            roadmap.edges.length +
            " dependencies · hover a card to isolate its path"
          : tracedTicket
            ? "Showing dependencies connected to " + tracedTicket
            : "Hover or focus a ticket to trace its dependencies · " +
              roadmap.edges.length +
              " total connections"}
      </div>
      <div ref={scrollContainer} className="min-h-0 flex-1 overflow-auto overscroll-contain p-5">
        <div className="relative" style={{ width, minHeight: height }}>
          <svg
            className="pointer-events-none absolute inset-0 z-0 overflow-visible"
            width={width}
            height={height}
            aria-hidden="true"
          >
            <defs>
              <marker
                id="roadmap-arrow"
                viewBox="0 0 10 10"
                refX="9"
                refY="5"
                markerWidth="5"
                markerHeight="5"
                orient="auto-start-reverse"
              >
                <path d="M 0 0 L 10 5 L 0 10 z" fill="var(--muted-foreground)" />
              </marker>
            </defs>
            {visibleEdges.map((edge) => {
              const from = positions.get(edge.from)
              const to = positions.get(edge.to)
              if (!from || !to || from.stage === to.stage) return null
              const startX = from.left + columnWidth
              const startY = from.top + cardHeight / 2
              const endX = to.left
              const endY = to.top + cardHeight / 2
              const control = Math.max(24, (endX - startX) / 2)
              const path = `M ${startX} ${startY} C ${startX + control} ${startY}, ${endX - control} ${endY}, ${endX} ${endY}`
              return (
                <path
                  key={`${edge.from}-${edge.to}`}
                  d={path}
                  fill="none"
                  stroke="var(--muted-foreground)"
                  strokeWidth={showAllConnections ? "1" : "1.75"}
                  strokeOpacity={showAllConnections ? "0.22" : "0.75"}
                  strokeDasharray={edge.context ? "4 3" : undefined}
                  markerEnd="url(#roadmap-arrow)"
                />
              )
            })}
          </svg>
          {roadmap.stages.map((stage) => (
            <div
              className="absolute top-0 z-10"
              key={stage.index}
              style={{ left: stage.index * (columnWidth + columnGap), width: columnWidth }}
            >
              <div
                className={cn(
                  "flex h-10 items-center gap-2 border-b text-[11px] text-muted-foreground",
                  stage.attention && "border-destructive/30 text-destructive",
                )}
              >
                {stage.attention ? (
                  <AlertTriangle className="size-3.5" />
                ) : stage.index === 0 ? (
                  <GitBranch className="size-3.5" />
                ) : (
                  <ArrowRight className="size-3.5" />
                )}
                <strong className="font-medium text-foreground">
                  {stage.attention
                    ? "Needs attention"
                    : stage.index === 0
                      ? "Start here"
                      : `Stage ${stage.index + 1}`}
                </strong>
                <span className="ml-auto">
                  {stage.tickets.length} {stage.tickets.length === 1 ? "ticket" : "parallel"}
                </span>
              </div>
              <div className="grid gap-3 pt-3">
                {stage.tickets.map((ticket) => (
                  <RoadmapCard
                    key={ticket.id}
                    ticket={ticket}
                    context={contextIds.has(ticket.id)}
                    density={density}
                    missing={roadmap.missing.get(ticket.id)}
                    unresolved={roadmap.unresolved.has(ticket.id)}
                    selected={selectedIds.has(ticket.id)}
                    focused={tracedTicket === ticket.id}
                    onFocusedTicket={setFocusedTicket}
                    onToggle={onToggle}
                    onSelect={onSelect}
                  />
                ))}
              </div>
            </div>
          ))}
        </div>
      </div>
    </section>
  )
}

function RoadmapCard({
  ticket,
  context,
  density,
  missing,
  unresolved,
  selected,
  focused,
  onFocusedTicket,
  onToggle,
  onSelect,
}: {
  ticket: Ticket
  context: boolean
  density: Density
  missing?: string[]
  unresolved: boolean
  selected: boolean
  focused: boolean
  onFocusedTicket: (id: string | null) => void
  onToggle: Props["onToggle"]
  onSelect: Props["onSelect"]
}) {
  const meta = statusMeta[ticket.status]
  const palette = roadmapStatusPalette(ticket.status)
  const StatusIcon = meta.icon
  return (
    <article
      data-ticket-id={ticket.id}
      tabIndex={0}
      className={cn(
        "group relative flex h-28 cursor-pointer flex-col rounded-lg border border-l-[3px] bg-card p-3 shadow-sm transition-[border-color,box-shadow] hover:border-ring/60 hover:shadow-md focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
        ticket.status === "closed" && "text-muted-foreground",
        unresolved && "border-destructive/35",
        (selected || focused) && "border-primary ring-1 ring-primary/40",
        density === "compact" && "p-2.5",
        context && "border-dashed bg-muted/25",
      )}
      style={{
        borderLeftColor: palette.color,
        backgroundColor: context
          ? `color-mix(in srgb, ${palette.background} 65%, var(--muted))`
          : palette.background,
      }}
      onMouseEnter={() => onFocusedTicket(ticket.id)}
      onMouseLeave={() => onFocusedTicket(null)}
      onFocus={() => onFocusedTicket(ticket.id)}
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget)) onFocusedTicket(null)
      }}
      onClick={(event) => {
        if ((event.target as HTMLElement).closest('[data-slot="checkbox"]')) return
        if (event.shiftKey) onToggle(ticket.id, true)
        else onSelect(ticket.id)
      }}
      onKeyDown={(event) => {
        if (event.key === "Enter") {
          event.preventDefault()
          onSelect(ticket.id)
        }
      }}
    >
      {ticket.status === "in_progress" && <InProgressCardPulse color={palette.color} />}
      <div className="flex min-w-0 items-center gap-2">
        <Checkbox
          checked={selected}
          className={selected ? "opacity-100" : "opacity-0 group-hover:opacity-100"}
          aria-label={`Select ${ticket.id}`}
          onClick={(event) => {
            event.stopPropagation()
            onToggle(ticket.id, event.shiftKey)
          }}
          onKeyDown={(event) => event.stopPropagation()}
        />
        <span className="truncate font-mono text-[10px] text-muted-foreground">{ticket.id}</span>
        <span
          className="ml-auto inline-flex h-5 shrink-0 items-center gap-1 rounded-full border px-1.5 text-[9px] font-medium"
          style={{
            color: palette.color,
            borderColor: `color-mix(in srgb, ${palette.color} 32%, var(--border))`,
            backgroundColor: `color-mix(in srgb, ${palette.color} 9%, var(--card))`,
          }}
        >
          <StatusIcon className="size-3" />
          {meta.label}
        </span>
        {context && <span className="text-[9px] text-muted-foreground">Context</span>}
      </div>
      <h3 className="mt-2 line-clamp-2 text-[13px] font-medium leading-[1.35]">{ticket.title}</h3>
      <div className="mt-auto flex min-w-0 items-center gap-1.5">
        <Priority priority={ticket.priority} />
        {ticket.deps.length > 0 && (
          <Badge variant="outline" className="px-1.5 text-[10px]">
            {ticket.deps.length} deps
          </Badge>
        )}
        {missing && (
          <Badge variant="destructive" className="max-w-28 px-1.5 text-[10px]">
            <Unplug /> <span className="truncate">Missing {missing.join(", ")}</span>
          </Badge>
        )}
        {unresolved && !missing && (
          <Badge variant="destructive" className="px-1.5 text-[10px]">
            <AlertTriangle /> Unresolved chain
          </Badge>
        )}
        <span className="ml-auto grid size-5 shrink-0 place-items-center rounded-full border bg-muted text-[8px] text-muted-foreground">
          {initials(ticket.assignee)}
        </span>
      </div>
    </article>
  )
}

function roadmapStatusPalette(status: Status) {
  const name = status.replace("_", "-")
  return {
    color: "var(--roadmap-" + name + ")",
    background: "var(--roadmap-" + name + "-surface)",
  }
}
