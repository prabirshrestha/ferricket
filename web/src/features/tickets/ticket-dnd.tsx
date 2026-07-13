import {
  DndContext,
  KeyboardSensor,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core"
import type { Status, Ticket } from "../../types/ticket"

type Props = {
  children: React.ReactNode
  tickets: Ticket[]
  onUpdate: (id: string, body: Partial<Ticket>) => Promise<void>
}

export function TicketDndProvider({ children, tickets, onUpdate }: Props) {
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor, {
      keyboardCodes: {
        start: ["Space"],
        cancel: ["Escape"],
        end: ["Space", "Enter"],
      },
    }),
  )

  const finishDrag = ({ active, over }: DragEndEvent) => {
    const id = String(active.id)
    const status = over?.data.current?.status as Status | undefined
    const ticket = tickets.find((item) => item.id === id)
    if (ticket && status && ticket.status !== status) {
      void onUpdate(id, { status })
    }
  }

  return (
    <DndContext
      sensors={sensors}
      accessibility={{
        screenReaderInstructions: {
          draggable:
            "Press space to pick up a ticket, use arrow keys to move it, then press space or enter to drop.",
        },
      }}
      onDragEnd={finishDrag}
    >
      {children}
    </DndContext>
  )
}

export const statusDropId = (status: Status) => `status:${status}`
