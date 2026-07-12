import { Monitor, Moon, Sun } from "lucide-react"
import { isTheme, type ResolvedTheme, type Theme } from "../../hooks/use-theme"
import { Button } from "../ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "../ui/dropdown-menu"

const options = [
  { value: "light", label: "Light", icon: Sun },
  { value: "dark", label: "Dark", icon: Moon },
  { value: "system", label: "System", icon: Monitor },
] as const satisfies ReadonlyArray<{ value: Theme; label: string; icon: typeof Sun }>

type Props = {
  theme: Theme
  resolvedTheme: ResolvedTheme
  onThemeChange: (theme: Theme) => void
}

export function ThemeMenu({ theme, resolvedTheme, onThemeChange }: Props) {
  const TriggerIcon = theme === "system" ? Monitor : resolvedTheme === "dark" ? Moon : Sun
  const label = options.find((option) => option.value === theme)?.label ?? "System"

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={`Theme: ${label}`}
            title={`Theme: ${label}`}
          />
        }
      >
        <TriggerIcon />
      </DropdownMenuTrigger>
      <DropdownMenuContent className="w-40" align="end" sideOffset={6}>
        <DropdownMenuRadioGroup
          value={theme}
          onValueChange={(value) => isTheme(value) && onThemeChange(value)}
        >
          <DropdownMenuLabel>Appearance</DropdownMenuLabel>
          {options.map(({ value, label: optionLabel, icon: Icon }) => (
            <DropdownMenuRadioItem key={value} value={value} closeOnClick>
              <Icon />
              {optionLabel}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
