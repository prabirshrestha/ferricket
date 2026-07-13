import dagre from "@dagrejs/dagre"
import type { Ticket } from "../../types/ticket"

export const dagNodeWidth = 244
export const dagNodeHeight = 108
export type DagDirection = "LR" | "TB"

export type DagConnection = { id: string; source: string; target: string }
export type DagPosition = { x: number; y: number }

export function dependencyConnections(
  tickets: Array<{ id: string; deps?: string[] }>,
): DagConnection[] {
  const known = new Set(tickets.map((ticket) => ticket.id))
  const seen = new Set<string>()
  const connections: DagConnection[] = []
  for (const ticket of tickets) {
    for (const dependency of new Set(ticket.deps ?? [])) {
      if (!known.has(dependency)) continue
      const id = dependency + "->" + ticket.id
      if (seen.has(id)) continue
      seen.add(id)
      connections.push({ id, source: dependency, target: ticket.id })
    }
  }
  return connections
}

export function dependencyTopologyKey(
  tickets: Array<{ id: string; deps?: string[] } & Record<string, unknown>>,
  connections = dependencyConnections(tickets),
): string {
  const ticketIds = tickets.map((ticket) => ticket.id).sort()
  const edges = connections.map(({ source, target }) => source + "\u001f" + target).sort()
  return ticketIds.join("\u001f") + "\u001e" + edges.join("\u001e")
}

export function prerequisitePath(ticketId: string | null, tickets: Ticket[]): Set<string> {
  if (!ticketId) return new Set()
  const byId = new Map(tickets.map((ticket) => [ticket.id, ticket]))
  if (!byId.has(ticketId)) return new Set()
  const included = new Set<string>()
  const pending = [ticketId]
  while (pending.length) {
    const id = pending.pop()!
    if (included.has(id)) continue
    included.add(id)
    const ticket = byId.get(id)
    if (!ticket) continue
    for (const dependency of ticket.deps) if (byId.has(dependency)) pending.push(dependency)
  }
  return included
}

export function layoutDependencyDag(
  tickets: Array<{ id: string; deps?: string[] }>,
  connections = dependencyConnections(tickets),
  direction: DagDirection = "LR",
): Map<string, DagPosition> {
  const graph = new dagre.graphlib.Graph()
  graph.setGraph({
    rankdir: direction,
    ranker: "tight-tree",
    nodesep: 18,
    edgesep: 10,
    ranksep: 52,
    marginx: 20,
    marginy: 20,
  })
  graph.setDefaultEdgeLabel(() => ({}))
  for (const ticket of tickets)
    graph.setNode(ticket.id, { width: dagNodeWidth, height: dagNodeHeight })
  for (const connection of connections) graph.setEdge(connection.source, connection.target)
  dagre.layout(graph)
  return new Map(
    tickets.map((ticket) => {
      const point = graph.node(ticket.id) as { x?: number; y?: number } | undefined
      return [
        ticket.id,
        {
          x: (point?.x ?? dagNodeWidth / 2) - dagNodeWidth / 2,
          y: (point?.y ?? dagNodeHeight / 2) - dagNodeHeight / 2,
        },
      ]
    }),
  )
}
