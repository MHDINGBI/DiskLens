export type IconName = "home" | "disk" | "clean" | "copy" | "package" | "residue" | "cloud" | "heart" | "report" | "spark" | "settings" | "sun" | "moon" | "arrow" | "refresh" | "search" | "shield" | "archive" | "registry" | "close";
const paths: Record<IconName, string[]> = {
  home: ["M3 10 12 3l9 7v10a1 1 0 0 1-1 1h-5v-7H9v7H4a1 1 0 0 1-1-1Z"],
  disk: ["M5 3h14l3 12v5a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1v-5Z", "M2 15h20", "M17 18h1"],
  clean: ["m14 3 7 7", "m17 6-9 9", "m8 11 5 5-4 5H3v-6Z", "m5 17 3 3"],
  copy: ["M9 9h12v12H9Z", "M15 9V3H3v12h6"],
  package: ["m12 3 9 5v9l-9 5-9-5V8Z", "m3 8 9 5 9-5", "M12 13v9", "m7 5 10 6"],
  residue: ["M7 3h10v4H7Z", "M5 7h14v14H5Z", "M9 11v6m6-6v6"],
  cloud: ["M6 19a5 5 0 0 1-1-10 7 7 0 0 1 13-2 6 6 0 0 1 0 12Z"],
  heart: ["M20.8 4.6a5.5 5.5 0 0 0-7.8 0L12 5.7l-1.1-1.1a5.5 5.5 0 0 0-7.8 7.8L12 21l8.8-8.6a5.5 5.5 0 0 0 0-7.8Z", "M3 12h5l2-4 4 8 2-4h5"],
  report: ["M5 3h10l4 4v14H5Z", "M15 3v5h4", "M9 17v-4m3 4v-7m3 7v-5"],
  spark: ["m12 3 3 6 6 3-6 3-3 6-3-6-6-3 6-3Z"],
  settings: ["M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8Z", "m9 3 6 0 .7 3 2.7 1.5 3-.9 2 5.2-2.4 2.1-.5 3 1.6 2.6-4.7 3.5-2.6-1.7h-3l-2.6 1.7-4.7-3.5 1.6-2.6-.5-3L.2 11.8l2-5.2 3 .9L8.3 6Z"],
  sun: ["M12 7a5 5 0 1 0 0 10 5 5 0 0 0 0-10Z", "M12 1v2m0 18v2M1 12h2m18 0h2M4 4l1.5 1.5m13 13L20 20M4 20l1.5-1.5m13-13L20 4"],
  moon: ["M20 15.5A9 9 0 0 1 8.5 4 9 9 0 1 0 20 15.5Z"],
  arrow: ["m9 5 7 7-7 7"], refresh: ["M20 7v5h-5", "M4 17v-5h5", "M6.1 6.1A8 8 0 0 1 20 12M4 12a8 8 0 0 0 13.9 5.9"],
  search: ["M10.5 3a7.5 7.5 0 1 0 0 15 7.5 7.5 0 0 0 0-15Z", "m16 16 5 5"],
  shield: ["m12 3 9 4v6c0 5-9 9-9 9s-9-4-9-9V7Z", "m8 12 3 3 5-6"],
  archive: ["M3 3h18v5H3Z", "M5 8v13h14V8", "M10 12h4"],
  registry: ["M3 3h6v6H3ZM15 3h6v6h-6ZM3 15h6v6H3ZM15 15h6v6h-6Z", "M9 6h6M6 9v6m12-6v6M9 18h6"],
  close: ["m6 6 12 12M6 18 18 6"],
};
export default function Icon({ name, size = 20 }: { name: IconName; size?: number }) {
  return <svg className="icon" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.65" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" focusable="false">{paths[name].map((d, i) => <path d={d} key={i} />)}</svg>;
}
