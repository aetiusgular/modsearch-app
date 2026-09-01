// Minimal inline icon set. Stroke uses currentColor. 20x20 grid.
type P = { size?: number; className?: string; fill?: boolean };
const base = (size = 18) => ({
  width: size, height: size, viewBox: "0 0 20 20", fill: "none",
  stroke: "currentColor", strokeWidth: 1.6, strokeLinecap: "round" as const, strokeLinejoin: "round" as const,
});

export const Icon = {
  ForYou: ({ size, className }: P) => (
    <svg {...base(size)} className={className}><path d="M10 2.5l2.1 4.6 5 .5-3.8 3.3 1.1 4.9L10 13.9 5.6 16.3l1.1-4.9L2.9 7.6l5-.5z" /></svg>
  ),
  Search: ({ size, className }: P) => (
    <svg {...base(size)} className={className}><circle cx="9" cy="9" r="5.5" /><path d="M13.5 13.5L17 17" /></svg>
  ),
  Saved: ({ size, className, fill }: P) => (
    <svg {...base(size)} className={className} fill={fill ? "currentColor" : "none"}><path d="M5.5 3.5h9v13l-4.5-3-4.5 3z" /></svg>
  ),
  Drops: ({ size, className }: P) => (
    <svg {...base(size)} className={className}><path d="M10 3v9" /><path d="M6 9l4 4 4-4" /><path d="M4 16.5h12" /></svg>
  ),
  Settings: ({ size, className }: P) => (
    <svg {...base(size)} className={className}><circle cx="10" cy="10" r="2.6" /><path d="M10 2.5v2M10 15.5v2M2.5 10h2M15.5 10h2M4.7 4.7l1.4 1.4M13.9 13.9l1.4 1.4M15.3 4.7l-1.4 1.4M6.1 13.9l-1.4 1.4" /></svg>
  ),
  Heart: ({ size, className, fill }: P) => (
    <svg {...base(size)} className={className} fill={fill ? "currentColor" : "none"}><path d="M10 16.5S3.5 12.6 3.5 7.9A3.4 3.4 0 0110 6.2a3.4 3.4 0 016.5 1.7c0 4.7-6.5 8.6-6.5 8.6z" /></svg>
  ),
  Bookmark: ({ size, className, fill }: P) => (
    <svg {...base(size)} className={className} fill={fill ? "currentColor" : "none"}><path d="M5.5 3.5h9v13l-4.5-3-4.5 3z" /></svg>
  ),
  Hide: ({ size, className }: P) => (
    <svg {...base(size)} className={className}><path d="M4 4l12 12M8.5 5.2A6.9 6.9 0 0110 5c4 0 6.5 3.6 6.5 5 0 .6-.5 1.6-1.4 2.6M11.6 11.7A2.2 2.2 0 018.3 8.4M5.2 6.6C4.2 7.6 3.5 8.7 3.5 10c0 1.4 2.5 5 6.5 5 .9 0 1.7-.2 2.4-.5" /></svg>
  ),
  Sliders: ({ size, className }: P) => (
    <svg {...base(size)} className={className}><path d="M4 6h8M4 14h5" /><circle cx="14" cy="6" r="2" /><circle cx="11" cy="14" r="2" /><path d="M16 14h0" /></svg>
  ),
  Close: ({ size, className }: P) => (
    <svg {...base(size)} className={className}><path d="M5 5l10 10M15 5L5 15" /></svg>
  ),
  External: ({ size, className }: P) => (
    <svg {...base(size)} className={className}><path d="M8 4H5.5A1.5 1.5 0 004 5.5v9A1.5 1.5 0 005.5 16h9a1.5 1.5 0 001.5-1.5V12M11 4h5v5M16 4l-7 7" /></svg>
  ),
  Sun: ({ size, className }: P) => (
    <svg {...base(size)} className={className}><circle cx="10" cy="10" r="3.2" /><path d="M10 2.5v2M10 15.5v2M2.5 10h2M15.5 10h2M4.7 4.7l1.4 1.4M13.9 13.9l1.4 1.4M15.3 4.7l-1.4 1.4M6.1 13.9l-1.4 1.4" /></svg>
  ),
  Moon: ({ size, className }: P) => (
    <svg {...base(size)} className={className}><path d="M15.5 11.5A6 6 0 018.5 4.5a6 6 0 103 7z" /></svg>
  ),
  Plus: ({ size, className }: P) => (
    <svg {...base(size)} className={className}><path d="M10 4v12M4 10h12" /></svg>
  ),
  Bolt: ({ size, className }: P) => (
    <svg {...base(size)} className={className}><path d="M11 2.5L4.5 11H9l-1 6.5L15.5 9H11z" /></svg>
  ),
};
