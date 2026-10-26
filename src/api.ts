import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { serialiserResidus } from "./sessionResidus";

export type Drive = { key: string; mount: string; label: string; total: number; free: number; scanned_at: number | null };
export type Item = { name: string; path: string; kind: "dir" | "file" | "small"; size: number; files: number; age_days: number | null };
export type Listing = { path: string; name: string; size: number; files: number; items: Item[]; hidden_count: number; hidden_size: number };
export type SvEtat = { id: string; titre: string; statut: "vert" | "orange" | "rouge" | "gris"; detail: string; age_jours: number | null; reglage: string | null };
export type Processus = { nom: string; chemin: string; instances: number; cpu: number; ram_octets: number; ram_pct: number; editeur: string; signe: boolean; classe: "connu" | "verifier" | "inconnu"; explication: string; raison: string };
export type ScanProcessus = { seuil: number; cpu_total: number; ram_totale: number; ram_utilisee: number; processus: Processus[] };
export type Protection = { id: string; titre: string; statut: "vert" | "orange" | "rouge" | "gris"; detail: string; reglage: string | null };
export type Connexion = { processus: string; chemin: string; pid: number; distant: string; port: number; classe: string; raison: string };
export type Extension = { navigateur: string; profil: string; id: string; nom: string; hors_store: boolean; permissions_risque: string[]; raison: string };
export type ScanSecurite = { connexions: Connexion[]; extensions: Extension[]; limites: string[] };
export type DemEntree = { id: string; nom: string; source: string; commande: string; chemin: string; editeur: string; signe: boolean; actif: boolean; suggestion: "garder" | "facultatif" | "inconnu"; note: string; admin: boolean };
export type DemResultat = { id: string; ok: boolean; detail: string };
export type Candidate = { id: string; category: string; path: string; size: number; risk: "safe" | "low" | "medium"; reason: string; age_days: number | null };
export type DupGroup = { size: number; wasted: number; paths: string[] };
export type DelResult = { path: string; ok: boolean; freed: number; error: string | null };
export type ScanSummary = { key: string; size: number; files: number; errors: number; secs: number; cancelled: boolean };
export type ScanProgress = { files: number; bytes: number; dirs: number; errors: number; current: string };
export type Provider = { id: string; name: string; kind: string; base_url: string; model: string; has_key: boolean };
export type Llama = { exe: string; models_dir: string; model_file: string; port: number; ctx: number };
export type Settings = { providers: Provider[]; active: string; llama: Llama };
export type Explain = { resume: string; risque: string; supprimable: boolean; se_regenere: boolean; conseil: string };
export type Pick = { id: string; path: string; size: number; category: string; risk: string; note: string };
export type ChatReply = { message: string; picks: Pick[] };
export type Msg = { role: "user" | "assistant"; content: string };
export type Growth = { path: string; size: number; delta: number };
export type ReportData = { key: string; history: [number, number, number][]; prev_ts: number | null; cur_ts: number | null; growth: Growth[] };
export type ResumeMensuel = { id: string; ts: number; markdown: string; html: string; alertes: string[] };
export type EtatMensuel = { active: boolean; derniere_execution: number | null; rapports: ResumeMensuel[]; dossier: string; seuil_libre_pct: number; planification: string };

export type Logiciel = { id: string; nom: string; editeur: string; version: string; install_location: string; desinstalleur: string; cle: string; etat: "installe" | "fantome"; raison: string; date_installation?: string | null; derniere_utilisation_estimee?: string | null; confiance_usage?: string; indice_usage?: string; origine_oem?: string | null; confiance_oem?: string };
export type Confiance = "sur" | "probable" | "verifier";
export type Residu = { id: string; logiciel_id: string; genre: "dossier" | "fichier"; chemin: string; octets: number; fichiers: number; confiance: Confiance; motif: string; protege: string | null };
export type PisteResidu = { nom: string; chemin: string; octets: number; fichiers: number; partiel: boolean; indices: string[] };
export type DecouverteResidus = { dossiers: PisteResidu[]; dossiers_examines: number; limites: string[] };
export type ResidusReport = { logiciels: Logiciel[]; residus: Residu[]; decouverte: DecouverteResidus | null };
export type QuarResult = { chemin: string; ok: boolean; detail: string; lot: string };
export type Lot = { nom: string; chemin: string; age_jours: number; octets: number };
export type CleReg = { id: string; logiciel_id: string; chemin: string; genre: string; confiance: Confiance; motif: string; protege: string | null; valeurs: number };
export type RegResult = { chemin: string; ok: boolean; detail: string; export: string | null };

export type OdDossier = { chemin: string; nom: string; local_octets: number; local_fichiers: number; epingle_octets: number; cloud_fichiers: number };
export type OdRacine = { chemin: string; compte: string; dossiers: OdDossier[]; local_octets: number; epingle_octets: number; cloud_fichiers: number };
export type OdResult = { chemin: string; ok: boolean; fichiers: number; octets: number; detail: string };

export const api = {
  mensuelEtat: () => invoke<EtatMensuel>("mensuel_etat"),
  mensuelGenerer: () => invoke<ResumeMensuel>("mensuel_generer"),
  mensuelPlanifier: (active: boolean, seuilLibrePct: number) => invoke<EtatMensuel>("mensuel_planifier", { active, seuilLibrePct }),
  mensuelOuvrir: (id: string, format: "md" | "html" | "json") => invoke<void>("mensuel_ouvrir", { id, format }),
  santeProcessus: (seuil: number) => invoke<ScanProcessus>("sante_processus", { seuil }),
  santeSecurite: () => invoke<ScanSecurite>("sante_securite"),
  demarrageListe: () => invoke<DemEntree[]>("demarrage_liste"),
  demarrageAppliquer: (ids: string[], activer: boolean) => invoke<DemResultat[]>("demarrage_appliquer", { ids, activer }),
  santeProtection: () => invoke<Protection[]>("sante_protection"),
  sauvegardeEtat: () => invoke<SvEtat[]>("sauvegarde_etat"),
  sauvegardeOuvrir: (reglage: string) => invoke<void>("sauvegarde_ouvrir", { reglage }),
  onedriveScan: () => invoke<OdRacine[]>("onedrive_scan"),
  onedriveLiberer: (chemins: string[], inclureEpingles: boolean) => invoke<OdResult[]>("onedrive_liberer", { chemins, inclureEpingles }),
  residusScan: () => serialiserResidus(() => invoke<ResidusReport>("residus_scan")),
  logicielsListe: () => invoke<Logiciel[]>("logiciels_liste"),
  logicielDesinstaller: (id: string, confirme: boolean) => invoke<string>("logiciel_desinstaller", { id, confirme }),
  logicielResidusApres: (id: string) => serialiserResidus(() => invoke<ResidusReport>("logiciel_residus_apres", { id })),
  residusChercher: (nom: string, editeur: string) => serialiserResidus(() => invoke<ResidusReport>("residus_chercher", { nom, editeur })),
  residusClean: (chemins: string[]) => serialiserResidus(() => invoke<QuarResult[]>("residus_clean", { chemins })),
  residusNettoyerEtScanner: (chemins: string[]) => serialiserResidus(async () => {
    const resultats = await invoke<QuarResult[]>("residus_clean", { chemins });
    try { return { resultats, rapport: await invoke<ResidusReport>("residus_scan"), erreur: "" }; }
    catch (e) { return { resultats, rapport: null, erreur: String(e) }; }
  }),
  residusLots: () => invoke<Lot[]>("residus_lots"),
  residusPurgeLot: (lot: string) => invoke<void>("residus_purge_lot", { lot }),
  registreScan: (logiciels: Logiciel[]) => serialiserResidus(() => invoke<CleReg[]>("registre_scan", { logiciels })),
  registrePointRestauration: () => invoke<string>("registre_point_restauration"),
  registreClean: (cles: string[], confirme: boolean) => serialiserResidus(() => invoke<RegResult[]>("registre_clean", { cles, confirme })),
  registreNettoyerEtScanner: (cles: string[], logiciels: Logiciel[]) => serialiserResidus(async () => {
    const resultats = await invoke<RegResult[]>("registre_clean", { cles, confirme: true });
    try { return { resultats, clesRestantes: await invoke<CleReg[]>("registre_scan", { logiciels }), erreur: "" }; }
    catch (e) { return { resultats, clesRestantes: null, erreur: String(e) }; }
  }),
  drives: () => invoke<Drive[]>("list_drives"),
  scan: (root: string) => invoke<ScanSummary>("start_scan", { root }),
  cancelScan: () => invoke<void>("cancel_scan"),
  top: (path: string, kind: "file" | "dir", limit = 20, ext?: string, minAgeDays?: number) =>
    invoke<Item[]>("top_items", { path, kind, limit, ext: ext || null, minAgeDays: minAgeDays ?? null }),
  children: (path: string, limit = 400) => invoke<Listing>("get_children", { path, limit }),
  reveal: (path: string) => invoke<void>("reveal", { path }),
  candidates: () => invoke<Candidate[]>("candidates"),
  duplicates: (minMb: number) => invoke<DupGroup[]>("duplicates", { minMb }),
  del: (items: { path: string; size: number }[], trash: boolean) => invoke<DelResult[]>("delete_paths", { items, trash }),
  protection: (paths: string[]) => invoke<(string | null)[]>("protection_check", { paths }),
  settings: () => invoke<Settings>("get_settings"),
  saveSettings: (providers: { id: string; base_url: string; model: string }[], active: string, llama: Llama) =>
    invoke<Settings>("save_settings", { providers, active, llama }),
  setKey: (id: string, key: string) => invoke<Settings>("set_key", { id, key }),
  test: (id: string) => invoke<string>("test_provider", { id }),
  llamaModels: (dir: string) => invoke<string[]>("llama_models", { dir }),
  llamaStart: () => invoke<string>("llama_start"),
  llamaStop: () => invoke<void>("llama_stop"),
  explain: (path: string) => invoke<Explain>("ai_explain", { path }),
  chat: (history: Msg[]) => invoke<ChatReply>("ai_chat", { history }),
  reportData: (key: string) => invoke<ReportData>("report_data", { key }),
  report: (key: string) => invoke<string>("ai_report", { key }),
  onProgress: (cb: (p: ScanProgress) => void) => listen<ScanProgress>("scan-progress", (e) => cb(e.payload)),
};

export function fmt(b: number): string {
  const u = ["o", "Ko", "Mo", "Go", "To"];
  let v = b, i = 0;
  while (v >= 1024 && i < 4) { v /= 1024; i++; }
  return `${v >= 100 || i === 0 ? v.toFixed(0) : v.toFixed(1)} ${u[i]}`;
}

export function ageLabel(d: number | null): string {
  if (d === null) return "—";
  if (d < 1) return "aujourd'hui";
  if (d < 60) return `${d} j`;
  if (d < 730) return `${Math.round(d / 30)} mois`;
  return `${(d / 365).toFixed(1)} ans`;
}
