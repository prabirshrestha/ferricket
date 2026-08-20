import { useCallback, useEffect, useState } from "react"
import { fetchSessionActivity } from "../api/session-activity"
import type { SessionActivity } from "../types/session"

const emptyActivity: SessionActivity = { events: [], warnings: [] }

export function useSessionActivity() {
  const [activity, setActivity] = useState<SessionActivity>(emptyActivity)
  const [error, setError] = useState("")

  const refresh = useCallback(async () => {
    try {
      setActivity(await fetchSessionActivity())
      setError("")
    } catch (reason) {
      setError(String(reason))
    }
  }, [])

  useEffect(() => {
    void refresh()
    const interval = window.setInterval(() => void refresh(), 5_000)
    return () => window.clearInterval(interval)
  }, [refresh])

  return { activity, error }
}
