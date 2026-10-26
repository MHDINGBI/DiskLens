import { useEffect, useState } from "react";
import { api, Settings as S } from "./api";

export default function Settings({ notify }: { notify: (m: string) => void }) {
  const [s, setS] = useState<S | null>(null);
  const [keys, setKeys] = useState<Record<string, string>>({});
  const [models, setModels] = useState<string[]>([]);
  const [test, setTest] = useState<Record<string, string>>({});
  const [loadError, setLoadError] = useState("");
  const [actionError, setActionError] = useState("");
  const [loadVersion, setLoadVersion] = useState(0);

  useEffect(() => {
    let active = true;
    setLoadError("");
    api.settings().then((value) => { if (active) setS(value); }).catch((e) => { if (active) setLoadError(String(e)); });
    return () => { active = false; };
  }, [loadVersion]);
  useEffect(() => {
    if (!s) return;
    api.llamaModels(s.llama.models_dir).then(setModels).catch((e) => { setModels([]); notify(String(e)); });
  }, [s?.llama.models_dir]); // eslint-disable-line
  if (!s) return <div className="page"><div className="empty">{loadError ? <><p>Impossible de charger les réglages : {loadError}</p><button className="btn" onClick={() => setLoadVersion((v) => v + 1)}>Réessayer</button></> : <p>Chargement des réglages…</p>}</div></div>;

  const upd = (id: string, f: "base_url" | "model", v: string) =>
    setS({ ...s, providers: s.providers.map((p) => (p.id === id ? { ...p, [f]: v } : p)) });
  const save = async (): Promise<boolean> => {
    setActionError("");
    try {
      setS(await api.saveSettings(s.providers.map(({ id, base_url, model }) => ({ id, base_url, model })), s.active, s.llama));
      notify("Réglages enregistrés");
      return true;
    } catch (e) { setActionError(String(e)); notify(String(e)); return false; }
  };
  const saveKey = async (id: string) => {
    setActionError("");
    try { setS(await api.setKey(id, keys[id] ?? "")); setKeys({ ...keys, [id]: "" }); notify("Clé enregistrée (chiffrée DPAPI, liée à ton compte Windows)"); }
    catch (e) { setActionError(String(e)); notify(String(e)); }
  };
  const run = async (id: string) => {
    if (!await save()) return;
    setTest({ ...test, [id]: "…" });
    try { const r = await api.test(id); setTest((t) => ({ ...t, [id]: "✅ " + r.slice(0, 60) })); }
    catch (e) { setTest((t) => ({ ...t, [id]: "❌ " + String(e).slice(0, 160) })); }
  };

  return (
    <div className="page">
      <div className="hero"><div><div className="big">Réglages IA</div><div className="muted">Seuls des noms, chemins, tailles et dates sont envoyés aux modèles — jamais le contenu des fichiers.</div></div>
        <button className="btn primary" onClick={save}>Enregistrer</button></div>
      {actionError && <div className="card rowitem bad">Action impossible : {actionError}. Vérifie les réglages et réessaie.</div>}

      {s.providers.map((p) => (
        <section key={p.id} className={"card" + (s.active === p.id ? " active" : "")}>
          <header>
            <label className="radio"><input type="radio" name="active" checked={s.active === p.id} onChange={() => setS({ ...s, active: p.id })} /> <h3>{p.name}</h3></label>
            <button className="link" onClick={() => run(p.id)}>Tester</button>
            <span className="muted small">{test[p.id]}</span>
          </header>
          {p.kind === "openai" && (
            <div className="grid2">
              <label>URL de base<input value={p.base_url} onChange={(e) => upd(p.id, "base_url", e.target.value)} /></label>
              <label>Modèle<input value={p.model} placeholder="(celui chargé)" onChange={(e) => upd(p.id, "model", e.target.value)} /></label>
              {(p.id === "deepseek" || p.id === "zai") && (
                <label className="span2">Clé API {p.has_key && <span className="pill safe">enregistrée</span>}
                  <div className="row"><input type="password" value={keys[p.id] ?? ""} placeholder={p.has_key ? "•••••• (laisser vide pour conserver)" : "sk-…"} onChange={(e) => setKeys({ ...keys, [p.id]: e.target.value })} />
                    <button className="btn" onClick={() => saveKey(p.id)}>Enregistrer la clé</button></div></label>
              )}
            </div>
          )}
          {p.kind !== "openai" && (
            <div className="grid2"><label>Modèle (optionnel)<input value={p.model} placeholder="par défaut de ton abonnement" onChange={(e) => upd(p.id, "model", e.target.value)} /></label>
              <div className="muted small">Utilise le CLI « {p.kind === "cli_claude" ? "claude" : "codex"} » installé et connecté à ton abonnement. Aucune clé à saisir.</div></div>
          )}
          {p.id === "llamacpp" && (
            <div className="grid2">
              <label className="span2">llama-server.exe<input value={s.llama.exe} placeholder="D:\…\llama-server.exe" onChange={(e) => setS({ ...s, llama: { ...s.llama, exe: e.target.value } })} /></label>
              <label>Dossier des modèles<input value={s.llama.models_dir} onChange={(e) => setS({ ...s, llama: { ...s.llama, models_dir: e.target.value } })} /></label>
              <label>Modèle .gguf<select value={s.llama.model_file} onChange={(e) => setS({ ...s, llama: { ...s.llama, model_file: e.target.value } })}><option value="">—</option>{models.map((m) => <option key={m}>{m}</option>)}</select></label>
              <label>Port<input type="number" value={s.llama.port} onChange={(e) => setS({ ...s, llama: { ...s.llama, port: +e.target.value } })} /></label>
              <label>Contexte<input type="number" value={s.llama.ctx} onChange={(e) => setS({ ...s, llama: { ...s.llama, ctx: +e.target.value } })} /></label>
              <div className="row span2">
                <button className="btn" onClick={async () => { if (!await save()) return; try { notify(await api.llamaStart()); } catch (e) { setActionError(String(e)); notify(String(e)); } }}>▶ Démarrer le serveur</button>
                <button className="btn" onClick={async () => { try { await api.llamaStop(); notify("Serveur arrêté"); } catch (e) { setActionError(String(e)); notify(String(e)); } }}>■ Arrêter</button>
              </div>
            </div>
          )}
        </section>
      ))}
    </div>
  );
}
