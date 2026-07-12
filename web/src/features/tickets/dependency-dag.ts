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

/**
 * Keep the smallest set of links that preserves dependency reachability.
 *
 * Ticket files sometimes repeat an ancestor as a direct dependency. Those
 * shortcut links are useful metadata, but feeding all of them to a layered
 * layout creates very long ranks and a thicket of lines. The returned
 * backbone still describes exactly the same prerequisite ordering.
 */
export function dependencyBackbone(
  tickets: Array<{ id: string }>,
  connections: DagConnection[],
): DagConnection[] {
  const outgoing = new Map(tickets.map((ticket) => [ticket.id, [] as string[]]))
  const incoming = new Map(tickets.map((ticket) => [ticket.id, 0]))
  for (const connection of connections) outgoing.get(connection.source)?.push(connection.target)
  for (const connection of connections)
    incoming.set(connection.target, (incoming.get(connection.target) ?? 0) + 1)

  // Transitive reduction is only unambiguous for a DAG. Preserve every link
  // when invalid ticket data contains a cycle so the problem remains visible.
  const ready = [...incoming].filter(([, count]) => count === 0).map(([id]) => id)
  let visitedCount = 0
  while (ready.length > 0) {
    const current = ready.pop()!
    visitedCount += 1
    for (const target of outgoing.get(current) ?? []) {
      const count = (incoming.get(target) ?? 0) - 1
      incoming.set(target, count)
      if (count === 0) ready.push(target)
    }
  }
  if (visitedCount !== tickets.length) return connections

  return connections.filter((connection) => {
    const pending = (outgoing.get(connection.source) ?? []).filter(
      (target) => target !== connection.target,
    )
    const visited = new Set(pending)
    while (pending.length > 0) {
      const current = pending.pop()!
      if (current === connection.target) return false
      for (const target of outgoing.get(current) ?? []) {
        if (visited.has(target)) continue
        visited.add(target)
        pending.push(target)
      }
    }
    return true
  })
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
    ranker: "network-simplex",
    acyclicer: "greedy",
    nodesep: 28,
    edgesep: 12,
    ranksep: 64,
    marginx: 20,
    marginy: 20,
  })
  graph.setDefaultEdgeLabel(() => ({}))
  for (const ticket of [...tickets].sort((left, right) => left.id.localeCompare(right.id)))
    graph.setNode(ticket.id, { width: dagNodeWidth, height: dagNodeHeight })
  for (const connection of [...connections].sort((left, right) => left.id.localeCompare(right.id)))
    graph.setEdge(connection.source, connection.target)
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
