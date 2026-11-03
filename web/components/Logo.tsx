/** The Methods mark: three kanban columns. */
export function Logo({ size = 28 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 32 32" aria-hidden focusable="false">
      <rect width="32" height="32" rx="8" fill="var(--gm-color-primary)" />
      <rect x="7" y="8" width="5" height="16" rx="2" fill="var(--gm-color-on-primary)" />
      <rect x="13.5" y="8" width="5" height="11" rx="2" fill="var(--gm-color-on-primary)" opacity="0.8" />
      <rect x="20" y="8" width="5" height="7" rx="2" fill="var(--gm-color-on-primary)" opacity="0.6" />
    </svg>
  );
}
