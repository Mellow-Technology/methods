/** The Methods mark: three kanban columns. Same as web/components/Logo.tsx. */
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

export function BotIcon({ size = 12 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" aria-hidden focusable="false">
      <rect x="2" y="5" width="12" height="9" rx="3" fill="none" stroke="currentColor" strokeWidth="1.5" />
      <circle cx="6" cy="9.5" r="1.2" fill="currentColor" />
      <circle cx="10" cy="9.5" r="1.2" fill="currentColor" />
      <path d="M8 2v3" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
    </svg>
  );
}
