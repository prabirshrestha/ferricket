/// <reference lib="webworker" />

import {
  layoutDependencyDag,
  type DagConnection,
  type DagDirection,
  type DagPosition,
} from "./dependency-dag"

export type DagLayoutRequest = {
  requestId: number
  tickets: Array<{ id: string }>
  connections: DagConnection[]
  direction: DagDirection
}

export type DagLayoutResponse = {
  requestId: number
  positions: Array<[string, DagPosition]>
}

self.onmessage = (event: MessageEvent<DagLayoutRequest>) => {
  const { requestId, tickets, connections, direction } = event.data
  const positions = layoutDependencyDag(tickets, connections, direction)
  self.postMessage({ requestId, positions: [...positions] } satisfies DagLayoutResponse)
}

export {}
