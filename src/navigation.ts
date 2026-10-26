import type { IconName } from "./Icon";
export const routes = {
  home: { label: "Vue d’ensemble", group: "Accueil", icon: "home" },
  explorer: { label: "Explorer le disque", group: "Stockage", icon: "disk" },
  clean: { label: "Nettoyage", group: "Stockage", icon: "clean" },
  dups: { label: "Doublons", group: "Stockage", icon: "copy" },
  onedrive: { label: "OneDrive", group: "Stockage", icon: "cloud" },
  logiciels: { label: "Logiciels", group: "Applications", icon: "package" },
  residus: { label: "Résidus", group: "Applications", icon: "residue" },
  sante: { label: "Santé du PC", group: "Suivi", icon: "heart" },
  report: { label: "Rapports", group: "Suivi", icon: "report" },
  ai: { label: "Assistant IA", group: "Outils", icon: "spark" },
  settings: { label: "Réglages", group: "Outils", icon: "settings" },
} satisfies Record<string, { label: string; group: string; icon: IconName }>;
export type Route = keyof typeof routes;
export function lireRoute(hash: string): Route {
  const cle = hash.replace(/^#/, "");
  return Object.prototype.hasOwnProperty.call(routes, cle) ? cle as Route : "home";
}
