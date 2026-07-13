import {
  Background,
  BackgroundVariant,
  Controls,
  Handle,
  MarkerType,
  MiniMap,
  Position,
  ReactFlow,
  type Edge,
  type Node,
  type NodeProps,
  type ReactFlowInstance,
} from "@xyflow/react"
import {
  ArrowDown,
  ArrowRight,
  ExternalLink,
  GitFork,
  Lock,
  Maximize2,
  Unlock,
  X,
} from "lucide-react"
import { useEffect, useMemo, useRef, useState } from "react"
import { Badge } from "../../components/ui/badge"
import { Button } from "../../components/ui/button"
import { Checkbox } from "../../components/ui/checkbox"
import { Toggle } from "../../components/ui/toggle"
import { ToggleGroup, ToggleGroupItem } from "../../components/ui/toggle-group"
import { CanvasFocusToggle } from "../../components/layout/canvas-focus-toggle"
import { cn } from "../../lib/utils"
import type { ResolvedTheme } from "../../hooks/use-theme"
import type { DagDirection } from "../../hooks/use-display-preferences"
import { statusMeta, type Status, type Ticket } from "../../types/ticket"
import {
  dagNodeHeight,
  dagNodeWidth,
  dependencyConnections,
  dependencyTopologyKey,
  type DagPosition,
  prerequisitePath,
} from "./dependency-dag"
import type { DagLayoutRequest, DagLayoutResponse } from "./dependency-layout-worker"
import { CanvasNodeFinder } from "./canvas-node-finder"
import { InProgressCardPulse } from "./in-progress-card-pulse"

declare const __FERRICKET_DAG_WORKER_URL__: string

type DagNodeData = Record<string, unknown> & {
  ticket: Ticket
  context: boolean
  focused: boolean
  inPath: boolean
  selected: boolean
  onOpen: (id: string) => void
  onToggle: (id: string, range: boolean) => void
  direction: DagDirection
}
type DagNode = Node<DagNodeData, "ticket">

type Props = {
  tickets: Ticket[]
  contextIds?: Set<string>
  selectedIds: Set<string>
  resolvedTheme: ResolvedTheme
  direction: DagDirection
  onDirectionChange: (direction: DagDirection) => void
  canvasFocused: boolean
  onCanvasFocusedChange: (focused: boolean) => void
  onToggle: (id: string, range: boolean) => void
  onSelect: (id: string) => void
}

const nodeTypes = { ticket: DependencyDagNode }

export function DependencyDagView({
  tickets,
  contextIds = new Set(),
  selectedIds,
  resolvedTheme,
  direction,
  onDirectionChange,
  canvasFocused,
  onCanvasFocusedChange,
  onToggle,
  onSelect,
}: Props) {
  const [focused, setFocused] = useState<string | null>(null)
  const [locked, setLocked] = useState(false)
  const [overview, setOverview] = useState(false)
  const [flow, setFlow] = useState<ReactFlowInstance<DagNode, Edge> | null>(null)
  const [positions, setPositions] = useState<Map<string, DagPosition>>(new Map())
  const [layoutError, setLayoutError] = useState("")
  const worker = useRef<Worker | null>(null)
  const requestId = useRef(0)
  const path = useMemo(() => prerequisitePath(focused, tickets), [focused, tickets])
  const graphTickets = useMemo(
    () => (!overview && focused ? tickets.filter((ticket) => path.has(ticket.id)) : tickets),
    [focused, overview, path, tickets],
  )
  const connections = useMemo(() => dependencyConnections(graphTickets), [graphTickets])
  const topologyKey = dependencyTopologyKey(graphTickets, connections)
  const layoutInput = useMemo(
    () => ({
      ticketIds: graphTickets.map((ticket) => ticket.id),
      connections,
    }),
    // Content-only ticket updates should not restart layout or move the camera.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [topologyKey],
  )
  useEffect(() => {
    const nextRequestId = requestId.current + 1
    requestId.current = nextRequestId
    setPositions(new Map())
    setLayoutError("")
    worker.current?.terminate()
    const layoutWorker = new Worker(__FERRICKET_DAG_WORKER_URL__, { type: "module" })
    worker.current = layoutWorker
    layoutWorker.onmessage = (event: MessageEvent<DagLayoutResponse>) => {
      if (event.data.requestId !== requestId.current) return
      setPositions(new Map(event.data.positions))
      setLayoutError("")
    }
    layoutWorker.onerror = () => {
      if (nextRequestId === requestId.current)
        setLayoutError("Dependency layout failed. Try changing direction or clearing focus.")
    }
    layoutWorker.postMessage({
      requestId: nextRequestId,
      tickets: layoutInput.ticketIds.map((id) => ({ id })),
      connections: layoutInput.connections,
      direction,
    } satisfies DagLayoutRequest)
    return () => {
      if (worker.current === layoutWorker) worker.current = null
      layoutWorker.terminate()
    }
  }, [direction, layoutInput])
  const nodes = useMemo<DagNode[]>(
    () =>
      graphTickets.map((ticket) => ({
        id: ticket.id,
        type: "ticket",
        position: positions.get(ticket.id) ?? { x: 0, y: 0 },
        width: dagNodeWidth,
        height: dagNodeHeight,
        data: {
          ticket,
          context: contextIds.has(ticket.id),
          focused: !overview && ticket.id === focused,
          inPath: path.has(ticket.id),
          selected: selectedIds.has(ticket.id),
          onOpen: onSelect,
          onToggle,
          direction,
        },
      })),
    [
      contextIds,
      direction,
      focused,
      graphTickets,
      onSelect,
      onToggle,
      overview,
      path,
      positions,
      selectedIds,
    ],
  )
  const edges = useMemo<Edge[]>(
    () =>
      connections.map((connection) => {
        const highlighted = Boolean(
          !overview && focused && path.has(connection.source) && path.has(connection.target),
        )
        return {
          ...connection,
          type: "smoothstep",
          animated: false,
          markerEnd: {
            type: MarkerType.ArrowClosed,
            width: 14,
            height: 14,
            color: highlighted ? "var(--dag-edge-focus)" : "var(--dag-edge)",
          },
          className: "transition-opacity",
          style: {
            strokeWidth: highlighted ? 1.8 : 1.1,
            stroke: highlighted ? "var(--dag-edge-focus)" : "var(--dag-edge)",
          },
        }
      }),
    [connections, focused, overview, path],
  )

  useEffect(() => {
    if (!flow || positions.size !== graphTickets.length) return
    const frame = requestAnimationFrame(() => {
      if (!overview && focused) {
        const position = positions.get(focused)
        if (position && graphTickets.length > 16) {
          void flow.setCenter(position.x + dagNodeWidth / 2, position.y + dagNodeHeight / 2, {
            zoom: 0.68,
            duration: 350,
          })
          return
        }
        void flow.fitView({
          padding: 0.14,
          duration: 350,
          minZoom: 0.12,
          maxZoom: 0.92,
        })
        return
      }
      void flow.fitView({
        padding: 0.15,
        duration: 350,
        minZoom: 0.05,
        maxZoom: 1,
      })
    })
    return () => cancelAnimationFrame(frame)
  }, [canvasFocused, direction, flow, focused, graphTickets.length, overview, positions])

  useEffect(() => {
    if (focused || overview || tickets.length === 0) return
    const initial =
      tickets.find((ticket) => ticket.status === "in_progress") ??
      tickets.find((ticket) => ticket.status === "open") ??
      tickets[0]
    if (initial) setFocused(initial.id)
  }, [focused, overview, tickets])

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
          "flex shrink-0 flex-wrap items-center gap-2 border-b px-4 py-2",
          canvasFocused && "px-3 py-1.5",
        )}
      >
        <div className="flex items-center gap-2 text-xs font-medium">
          <GitFork className="size-4 text-muted-foreground" /> Dependency DAG
        </div>
        <p className="text-[11px] text-muted-foreground">
          Click to focus prerequisites · double-click to open · Fit View shows the complete path
        </p>
        <div className="ml-auto flex items-center gap-1.5 text-[11px] text-muted-foreground">
          {(["open", "in_progress", "closed"] as const).map((status) => {
            const meta = statusMeta[status]
            const palette = dagStatusPalette(status)
            const StatusIcon = meta.icon
            return (
              <span
                key={status}
                className="inline-flex h-7 items-center gap-1.5 rounded-md border bg-card px-2"
              >
                <StatusIcon className="size-3.5" color={palette.color} />
                {meta.label}{" "}
                <strong className="font-medium text-foreground">{statusCounts[status]}</strong>
              </span>
            )
          })}
          {focused && (
            <Button
              variant="outline"
              size="sm"
              onClick={() => {
                setFocused(null)
                setOverview(true)
              }}
            >
              <X /> Clear focus
            </Button>
          )}
          <Button
            variant={overview ? "secondary" : "outline"}
            size="sm"
            onClick={() => setOverview((value) => !value)}
          >
            <Maximize2 /> {overview ? "Focused path" : "Full graph"}
          </Button>
          <ToggleGroup
            aria-label="DAG direction"
            value={[direction]}
            onValueChange={(values) => {
              const next = values.at(-1)
              if (next === "LR" || next === "TB") onDirectionChange(next)
            }}
            variant="outline"
            size="sm"
            spacing={0}
          >
            <ToggleGroupItem value="LR" aria-label="Left to right layout">
              <ArrowRight />
            </ToggleGroupItem>
            <ToggleGroupItem value="TB" aria-label="Top to bottom layout">
              <ArrowDown />
            </ToggleGroupItem>
          </ToggleGroup>
          <Toggle
            size="sm"
            variant="outline"
            pressed={locked}
            onPressedChange={setLocked}
            aria-label={locked ? "Unlock graph navigation" : "Lock graph navigation"}
          >
            {locked ? <Lock /> : <Unlock />}
          </Toggle>
          <CanvasNodeFinder
            tickets={tickets}
            onLocate={(id) => {
              setOverview(false)
              setFocused(id)
            }}
          />
          <CanvasFocusToggle focused={canvasFocused} onFocusedChange={onCanvasFocusedChange} />
        </div>
      </header>
      <div className="relative min-h-0 flex-1">
        <ReactFlow<DagNode, Edge>
          nodes={nodes}
          edges={edges}
          nodeTypes={nodeTypes}
          onInit={setFlow}
          onNodeClick={(event, node) => {
            if ((event.target as Element).closest("[data-dag-node-action]")) return
            const clearing = focused === node.id
            setOverview(clearing)
            setFocused(clearing ? null : node.id)
          }}
          onNodeDoubleClick={(_, node) => onSelect(node.id)}
          onPaneClick={() => {
            setFocused(null)
            setOverview(true)
          }}
          nodesDraggable={false}
          nodesConnectable={false}
          elementsSelectable={!locked}
          panOnDrag={!locked}
          zoomOnScroll={!locked}
          zoomOnPinch={!locked}
          zoomOnDoubleClick={!locked}
          onlyRenderVisibleElements
          minZoom={0.05}
          maxZoom={1.6}
          fitView
          fitViewOptions={{ padding: 0.15, maxZoom: 1 }}
          colorMode={resolvedTheme}
          className="bg-background"
        >
          <Background variant={BackgroundVariant.Dots} gap={20} size={1} color="var(--border)" />
          <MiniMap
            pannable={!locked}
            zoomable={!locked}
            nodeColor={(node) => {
              const data = node.data as DagNodeData
              return dagStatusPalette(data.ticket.status).color
            }}
            nodeStrokeColor={(node) => {
              const data = node.data as DagNodeData
              return dagStatusPalette(data.ticket.status).color
            }}
            maskColor="color-mix(in srgb, var(--background) 78%, transparent)"
            className="!border !border-border !bg-card"
          />
          <Controls
            showInteractive={false}
            className="!overflow-hidden !rounded-lg !border !border-border !bg-card !shadow-sm [&_button]:!border-border [&_button]:!bg-card [&_button]:!fill-foreground"
          />
        </ReactFlow>
        {positions.size !== graphTickets.length && !layoutError && (
          <div className="pointer-events-none absolute inset-0 grid place-items-center bg-background/70 text-xs text-muted-foreground backdrop-blur-[1px]">
            Laying out {graphTickets.length} tickets…
          </div>
        )}
        {layoutError && (
          <div className="absolute inset-x-4 top-4 rounded-md border border-destructive/40 bg-background px-3 py-2 text-xs text-destructive shadow-sm">
            {layoutError}
          </div>
        )}
      </div>
    </section>
  )
}

function DependencyDagNode({ data }: NodeProps<DagNode>) {
  const { ticket } = data
  const meta = statusMeta[ticket.status]
  const palette = dagStatusPalette(ticket.status)
  const StatusIcon = meta.icon
  return (
    <article
      className={cn(
        "group relative flex h-[108px] w-[244px] flex-col rounded-lg border border-l-[3px] bg-card p-3 text-foreground shadow-[0_1px_2px_rgb(0_0_0/0.05)] transition-[border-color,box-shadow,opacity]",
        data.context && "border-dashed bg-muted/30",
        (data.focused || data.selected) && "border-primary shadow-md ring-2 ring-primary/30",
        ticket.blocked && "border-destructive/50",
      )}
      style={{
        borderLeftColor: palette.color,
        backgroundColor: data.context
          ? `color-mix(in srgb, ${palette.background} 65%, var(--muted))`
          : palette.background,
      }}
    >
      {ticket.status === "in_progress" && <InProgressCardPulse color={palette.color} />}
      <Handle
        type="target"
        position={data.direction === "TB" ? Position.Top : Position.Left}
        className="!size-2 !border-background !bg-muted-foreground"
      />
      <div className="flex min-w-0 items-center gap-2">
        <span
          className="nodrag nopan inline-flex"
          data-dag-node-action
          onClick={(event) => event.stopPropagation()}
        >
          <Checkbox
            checked={data.selected}
            aria-label={"Select " + ticket.id}
            onClick={(event) => data.onToggle(ticket.id, event.shiftKey)}
          />
        </span>
        <StatusIcon className="size-3.5 shrink-0" color={palette.color} />
        <span className="truncate font-mono text-[10px] text-muted-foreground">{ticket.id}</span>
        <span className="ml-auto text-[10px] text-muted-foreground">{meta.label}</span>
      </div>
      <h3 className="mt-2 line-clamp-2 text-[13px] font-medium leading-[1.35]">{ticket.title}</h3>
      <div className="mt-auto flex items-center gap-1.5">
        <Badge variant="outline" className="px-1.5 text-[10px] text-muted-foreground">
          P{ticket.priority}
        </Badge>
        {ticket.deps.length > 0 && (
          <Badge variant="outline" className="px-1.5 text-[10px]">
            {ticket.deps.length} deps
          </Badge>
        )}
        {data.context && <span className="text-[9px] text-muted-foreground">Context</span>}
        <Button
          variant="ghost"
          size="icon-xs"
          data-dag-node-action
          className="nodrag nopan ml-auto opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
          aria-label={"Open " + ticket.id}
          onClick={(event) => {
            event.stopPropagation()
            data.onOpen(ticket.id)
          }}
        >
          <ExternalLink />
        </Button>
      </div>
      <Handle
        type="source"
        position={data.direction === "TB" ? Position.Bottom : Position.Right}
        className="!size-2 !border-background !bg-muted-foreground"
      />
    </article>
  )
}

function dagStatusPalette(status: Status) {
  const name = status.replace("_", "-")
  return {
    color: "var(--dag-" + name + ")",
    background: "var(--dag-" + name + "-surface)",
  }
}
