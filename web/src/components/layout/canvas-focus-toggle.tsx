import { Maximize, Minimize2 } from "lucide-react"
import { Button } from "../ui/button"

export function CanvasFocusToggle({
  focused,
  onFocusedChange,
}: {
  focused: boolean
  onFocusedChange: (focused: boolean) => void
}) {
  const label = focused ? "Exit full screen" : "Enter full screen"
  return (
    <Button
      variant={focused ? "secondary" : "outline"}
      size="icon-sm"
      aria-label={label}
      aria-pressed={focused}
      title={focused ? `${label} (Esc)` : label}
      onClick={() => onFocusedChange(!focused)}
    >
      {focused ? <Minimize2 /> : <Maximize />}
    </Button>
  )
}
