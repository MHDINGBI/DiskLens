import { Item, fmt } from "./api";

type Rect = { x: number; y: number; w: number; h: number; item: Item };

// Squarified treemap (Bruls et al.)
function layout(items: Item[], W: number, H: number): Rect[] {
  const total = items.reduce((s, i) => s + i.size, 0);
  if (total <= 0 || W <= 0 || H <= 0) return [];
  const scale = (W * H) / total;
  const nodes = items.filter((i) => i.size > 0).map((i) => ({ item: i, area: i.size * scale }));
  const out: Rect[] = [];
  let x = 0, y = 0, w = W, h = H;
  let row: typeof nodes = [];

  const worst = (r: typeof nodes, side: number) => {
    const s = r.reduce((a, n) => a + n.area, 0);
    const mx = Math.max(...r.map((n) => n.area)), mn = Math.min(...r.map((n) => n.area));
    return Math.max((side * side * mx) / (s * s), (s * s) / (side * side * mn));
  };
  const flush = () => {
    if (!row.length) return;
    const s = row.reduce((a, n) => a + n.area, 0);
    if (w >= h) {
      const rw = s / h; let cy = y;
      for (const n of row) { const rh = n.area / rw; out.push({ x, y: cy, w: rw, h: rh, item: n.item }); cy += rh; }
      x += rw; w -= rw;
    } else {
      const rh = s / w; let cx = x;
      for (const n of row) { const rw = n.area / rh; out.push({ x: cx, y, w: rw, h: rh, item: n.item }); cx += rw; }
      y += rh; h -= rh;
    }
    row = [];
  };
  for (const n of nodes) {
    const side = Math.min(w, h);
    if (row.length && worst([...row, n], side) > worst(row, side)) flush();
    row.push(n);
  }
  flush();
  return out;
}

export function colorFor(it: Item): string {
  if (it.kind === "small") return "hsl(220 10% 28%)";
  let h = 0;
  for (let i = 0; i < it.name.length; i++) h = (h * 31 + it.name.charCodeAt(i)) % 360;
  return it.kind === "file" ? `hsl(${h} 45% 28%)` : `hsl(${h} 55% 30%)`;
}

export default function Treemap(p: {
  items: Item[]; width: number; height: number;
  selected: Set<string>; onOpen: (i: Item) => void; onToggle: (i: Item) => void; onHover: (i: Item | null) => void;
}) {
  const rects = layout(p.items.slice(0, 120), p.width, p.height);
  return (
    <div className="treemap" style={{ width: p.width, height: p.height }}>
      {rects.map((r) => {
        const sel = p.selected.has(r.item.path) && r.item.kind !== "small";
        const big = r.w > 70 && r.h > 34;
        return (
          <div
            key={r.item.path + r.item.name}
            className={"tile" + (sel ? " sel" : "")}
            style={{ left: r.x, top: r.y, width: Math.max(r.w - 2, 0), height: Math.max(r.h - 2, 0), background: colorFor(r.item) }}
            onClick={(e) => (e.ctrlKey || e.metaKey ? p.onToggle(r.item) : r.item.kind === "dir" ? p.onOpen(r.item) : p.onToggle(r.item))}
            onContextMenu={(e) => { e.preventDefault(); p.onToggle(r.item); }}
            onMouseEnter={() => p.onHover(r.item)}
            onMouseLeave={() => p.onHover(null)}
          >
            {big && (
              <>
                <span className="tname">{r.item.kind === "dir" ? "📁 " : ""}{r.item.name}</span>
                <span className="tsize">{fmt(r.item.size)}</span>
              </>
            )}
          </div>
        );
      })}
    </div>
  );
}
