import { useCallback, useEffect, useState } from "react"

export type NavigateOptions = { replace?: boolean }
export type Navigate = (destination: string | number, options?: NavigateOptions) => void

export function useBrowserLocation() {
  const [pathname, setPathname] = useState(() => window.location.pathname)

  useEffect(() => {
    const onPopState = () => setPathname(window.location.pathname)
    window.addEventListener("popstate", onPopState)
    return () => window.removeEventListener("popstate", onPopState)
  }, [])

  const navigate = useCallback<Navigate>((destination, options) => {
    if (typeof destination === "number") {
      window.history.go(destination)
      return
    }
    const method = options?.replace ? "replaceState" : "pushState"
    window.history[method](null, "", destination)
    setPathname(window.location.pathname)
  }, [])

  return { pathname, navigate }
}
