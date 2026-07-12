export function InProgressCardPulse({ color }: { color: string }) {
  return (
    <span
      aria-hidden="true"
      className="pointer-events-none absolute inset-0 rounded-[inherit] motion-safe:animate-[pulse_2.4s_ease-in-out_infinite]"
      style={{
        boxShadow: `inset 0 0 0 1px color-mix(in srgb, ${color} 34%, transparent), 0 0 18px color-mix(in srgb, ${color} 13%, transparent)`,
      }}
    />
  )
}
