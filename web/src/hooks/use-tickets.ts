import { useCallback, useEffect, useState } from "react"
import { fetchMeta, fetchTickets, updateTicket } from "../api/tickets"
import { ApiError } from "../api/client"
import type { Meta, Ticket } from "../types/ticket"

export function useTickets() {
  const [tickets, setTickets] = useState<Ticket[]>([])
  const [meta, setMeta] = useState<Meta>({
    tickets_dir: ".tickets",
    workspace: "",
    version: "",
    watch_enabled: true,
  })
  const [error, setError] = useState("")
  const [live, setLive] = useState(true)
  const [pendingChanges, setPendingChanges] = useState(0)

  const refresh = useCallback(async () => {
    try {
      const [nextTickets, nextMeta] = await Promise.all([fetchTickets(), fetchMeta()])
      setTickets(nextTickets)
      setMeta(nextMeta)
      setError("")
    } catch (reason) {
      setError(String(reason))
    }
  }, [])

  useEffect(() => {
    void refresh()
  }, [refresh])

  useEffect(() => {
    if (!meta.watch_enabled) return
    const source = new EventSource("/api/events")
    source.addEventListener("tickets", () => {
      if (live) void refresh()
      else setPendingChanges((count) => count + 1)
    })
    source.onerror = () =>
      setError("Live updates disconnected; Ferricket will retry automatically.")
    return () => source.close()
  }, [live, meta.watch_enabled, refresh])

  const resumeLive = useCallback(() => {
    setLive(true)
    setPendingChanges(0)
    void refresh()
  }, [refresh])

  const update = useCallback(
    async (id: string, body: Partial<Ticket>) => {
      try {
        const current = tickets.find((item) => item.id === id)
        if (!current) throw new Error(`Ticket ${id} is no longer available.`)
        const changed = await updateTicket(id, { ...body, revision: current.revision })
        setTickets((items) => items.map((item) => (item.id === id ? changed : item)))
      } catch (reason) {
        if (reason instanceof ApiError && reason.status === 412) {
          setError(
            "This ticket changed on disk. Your edit was not applied; the latest version has been loaded.",
          )
          if (reason.latest)
            setTickets((items) =>
              items.map((item) => (item.id === id ? (reason.latest as Ticket) : item)),
            )
        } else setError(String(reason))
      }
    },
    [tickets],
  )

  const add = useCallback((ticket: Ticket) => setTickets((items) => [...items, ticket]), [])

  return {
    tickets,
    meta,
    error,
    setError,
    refresh,
    update,
    add,
    live,
    setLive,
    resumeLive,
    pendingChanges,
    setTickets,
  }
}
