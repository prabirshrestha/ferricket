"use client"

import * as React from "react"
import { ChevronDown, ChevronLeft, ChevronRight } from "lucide-react"
import { DayPicker, getDefaultClassNames, type DayButton } from "react-day-picker"
import { cn } from "../../lib/utils"
import { Button, buttonVariants } from "./button"

function Calendar({
  className,
  classNames,
  showOutsideDays = true,
  captionLayout = "label",
  components,
  ...props
}: React.ComponentProps<typeof DayPicker>) {
  const defaults = getDefaultClassNames()
  return (
    <DayPicker
      showOutsideDays={showOutsideDays}
      captionLayout={captionLayout}
      className={cn(
        "group/calendar w-fit bg-transparent p-2 [--cell-size:--spacing(8)]",
        className,
      )}
      classNames={{
        root: cn("w-fit", defaults.root),
        months: cn("relative flex flex-col", defaults.months),
        month: cn("flex w-full flex-col gap-3", defaults.month),
        nav: cn("absolute inset-x-0 top-0 flex items-center justify-between", defaults.nav),
        button_previous: cn(
          buttonVariants({ variant: "ghost", size: "icon-sm" }),
          defaults.button_previous,
        ),
        button_next: cn(
          buttonVariants({ variant: "ghost", size: "icon-sm" }),
          defaults.button_next,
        ),
        month_caption: cn(
          "flex h-8 items-center justify-center px-8 text-sm font-medium",
          defaults.month_caption,
        ),
        dropdowns: cn("flex items-center justify-center gap-1", defaults.dropdowns),
        dropdown_root: cn("relative rounded-md", defaults.dropdown_root),
        dropdown: cn("absolute inset-0 opacity-0", defaults.dropdown),
        caption_label: cn("flex items-center gap-1 text-sm font-medium", defaults.caption_label),
        month_grid: cn("w-full border-collapse", defaults.month_grid),
        weekdays: cn("flex", defaults.weekdays),
        weekday: cn("w-8 text-center text-xs font-normal text-muted-foreground", defaults.weekday),
        week: cn("mt-1 flex w-full", defaults.week),
        day: cn("relative size-8 p-0 text-center", defaults.day),
        today: cn("rounded-md bg-muted text-foreground", defaults.today),
        outside: cn("text-muted-foreground opacity-45", defaults.outside),
        disabled: cn("text-muted-foreground opacity-35", defaults.disabled),
        hidden: cn("invisible", defaults.hidden),
        ...classNames,
      }}
      components={{
        Chevron: ({ className, orientation }) => {
          const Icon =
            orientation === "left"
              ? ChevronLeft
              : orientation === "right"
                ? ChevronRight
                : ChevronDown
          return <Icon className={cn("size-4", className)} />
        },
        DayButton: CalendarDayButton,
        ...components,
      }}
      {...props}
    />
  )
}

function CalendarDayButton({
  className,
  day,
  modifiers,
  ...props
}: React.ComponentProps<typeof DayButton>) {
  const ref = React.useRef<HTMLButtonElement>(null)
  React.useEffect(() => {
    if (modifiers.focused) ref.current?.focus()
  }, [modifiers.focused])
  return (
    <Button
      ref={ref}
      variant="ghost"
      size="icon-sm"
      data-day={day.date.toLocaleDateString()}
      data-selected={modifiers.selected}
      className={cn(
        "size-8 rounded-md font-normal data-[selected=true]:bg-primary data-[selected=true]:text-primary-foreground",
        className,
      )}
      {...props}
    />
  )
}

export { Calendar, CalendarDayButton }
