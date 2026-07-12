import { useEffect, useState } from "react"
import { fetchPreferences, savePreferences } from "../api/preferences"

export type Theme = "light" | "dark" | "system"
export type ResolvedTheme = Exclude<Theme, "system">

const storageKey = "ferricket-theme"
const systemThemeQuery = "(prefers-color-scheme: dark)"
export const defaultTheme: Theme = "system"

export function isTheme(value: unknown): value is Theme {
  return value === "light" || value === "dark" || value === "system"
}

function storedTheme(): Theme {
  try {
    const value = localStorage.getItem(storageKey)
    return isTheme(value) ? value : defaultTheme
  } catch {
    return defaultTheme
  }
}

function currentSystemTheme(): ResolvedTheme {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") return "light"
  return window.matchMedia(systemThemeQuery).matches ? "dark" : "light"
}

export function useTheme() {
  const [theme, setTheme] = useState<Theme>(storedTheme)
  const [systemTheme, setSystemTheme] = useState<ResolvedTheme>(currentSystemTheme)
  const [hydrated, setHydrated] = useState(false)
  const resolvedTheme = theme === "system" ? systemTheme : theme

  useEffect(() => {
    let active = true
    void fetchPreferences("theme")
      .then((value) => {
        if (!active || !value || typeof value !== "object") return
        const persisted = (value as { theme?: unknown }).theme
        if (isTheme(persisted)) setTheme(persisted)
      })
      .catch(() => {
        /* Keep the browser fallback. */
      })
      .finally(() => {
        if (active) setHydrated(true)
      })
    return () => {
      active = false
    }
  }, [])

  useEffect(() => {
    if (typeof window.matchMedia !== "function") return
    const media = window.matchMedia(systemThemeQuery)
    const updateSystemTheme = () => setSystemTheme(media.matches ? "dark" : "light")
    updateSystemTheme()
    if (typeof media.addEventListener === "function") {
      media.addEventListener("change", updateSystemTheme)
      return () => media.removeEventListener("change", updateSystemTheme)
    }
    media.addListener(updateSystemTheme)
    return () => media.removeListener(updateSystemTheme)
  }, [])

  useEffect(() => {
    const syncTheme = (event: StorageEvent) => {
      if (event.key === storageKey)
        setTheme(isTheme(event.newValue) ? event.newValue : defaultTheme)
    }
    window.addEventListener("storage", syncTheme)
    return () => window.removeEventListener("storage", syncTheme)
  }, [])

  useEffect(() => {
    const root = document.documentElement
    root.classList.toggle("dark", resolvedTheme === "dark")
    root.dataset.theme = theme
    root.style.colorScheme = resolvedTheme
    document
      .querySelector<HTMLMetaElement>('meta[name="theme-color"]')
      ?.setAttribute("content", resolvedTheme === "dark" ? "#0b0b0d" : "#f7f8fa")
    try {
      localStorage.setItem(storageKey, theme)
    } catch {
      // The selected theme still applies when storage is unavailable.
    }
    const timeout = hydrated
      ? window.setTimeout(() => {
          void savePreferences("theme", { theme }).catch(() => {
            /* Browser fallback remains available. */
          })
        }, 200)
      : undefined
    return () => {
      if (timeout !== undefined) window.clearTimeout(timeout)
    }
  }, [hydrated, resolvedTheme, theme])

  const selectTheme = (value: unknown) => {
    if (isTheme(value)) setTheme(value)
  }

  return { theme, resolvedTheme, setTheme: selectTheme }
}
