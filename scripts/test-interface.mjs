import { build } from "esbuild";
import { createRequire } from "node:module";
import { mkdir, readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import test from "node:test";
import assert from "node:assert/strict";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";

const sortie = new URL("../Journaux/tests-interface/", import.meta.url);
await mkdir(sortie, { recursive: true });
await build({ entryPoints: ["src/App.tsx", "src/Residus.tsx", "src/Views.tsx", "src/Treemap.tsx", "src/sessionResidus.ts"], bundle: true, platform: "node", format: "cjs",
  outdir: fileURLToPath(sortie), outExtension: { ".js": ".cjs" }, external: ["react", "react/*", "react-dom", "react-dom/*"], logLevel: "silent" });
const require = createRequire(import.meta.url);
globalThis.localStorage = { getItem: () => "light", setItem() {}, removeItem() {} };
const App = require(fileURLToPath(new URL("App.cjs", sortie))).default;

test("l'accueil donne accès à la désinstallation sans analyse préalable", () => {
  const html = renderToStaticMarkup(React.createElement(App));
  assert.match(html, /Désinstaller un logiciel/);
  assert.match(html, /Navigation principale/);
  assert.match(html, /href="#logiciels"/);
});

test("le lien Logiciels ouvre son inventaire sans exiger de scan des résidus", () => {
  globalThis.window = { location: { hash: "#logiciels" } };
  try {
    const html = renderToStaticMarkup(React.createElement(App));
    assert.match(html, /Vos logiciels, au même endroit/);
    assert.match(html, /Rechercher un logiciel ou un éditeur/);
    assert.match(html, /sans recherche de résidus/);
    assert.doesNotMatch(html, /Retrouver les traces d’anciens logiciels/);
  } finally { delete globalThis.window; }
});

test("les prérequis de nettoyage et de doublons proposent une action pour continuer", () => {
  const { Clean, Dups } = require(fileURLToPath(new URL("Views.cjs", sortie)));
  for (const Page of [Clean, Dups]) {
    const html = renderToStaticMarkup(React.createElement(Page, { selected: new Map(), toggle() {}, setMany() {}, drivesReady: false }));
    assert.match(html, /href="#explorer"/);
  }
});

test("les pistes sont paginées et les opérations avancées séparées du résultat courant", () => {
  const Residus = require(fileURLToPath(new URL("Residus.cjs", sortie))).default;
  const report = { logiciels: [], residus: [], decouverte: { dossiers_examines: 45, limites: [], dossiers: Array.from({ length: 45 }, (_, i) => ({ nom: `Ancien outil ${i}`, chemin: `C:\\Fixture\\Trace${i}`, octets: 1024, fichiers: 1, partiel: false, indices: [] })) } };
  const html = renderToStaticMarkup(React.createElement(Residus, { notify() {}, rapport: report, onRapport() {} }));
  assert.match(html, /Ancien outil 29/);
  assert.doesNotMatch(html, /Ancien outil 30/);
  assert.match(html, /section hidden="" class="card rs-card rs-reg-section"/);
  assert.match(html, /section hidden="" class="card rs-card rs-quarantine-section"/);
  assert.doesNotMatch(html, / checked=""/);
});

const tokens = await readFile(new URL("../src/tokens.css", import.meta.url), "utf8");
const declarations = (s) => Object.fromEntries(Array.from(s.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g), (m) => [m[1], m[2].trim()]));
const [sombre, clair] = tokens.split(":root[data-theme=light]");
const couleurs = (theme) => {
  const valeurs = { ...declarations(sombre), ...(theme === "light" ? declarations(clair) : {}) };
  const valeur = (nom, profondeur = 0) => {
    assert.ok(profondeur < 10, `Alias cyclique : ${nom}`);
    const v = valeurs[nom];
    assert.ok(v, `Token absent : ${nom}`);
    return v.startsWith("var(") ? valeur(v.slice(4, -1), profondeur + 1) : v;
  };
  return valeur;
};
function luminance(hex) {
  assert.match(hex, /^#[0-9a-f]{6}$/i);
  const c = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255).map((v) => v <= .04045 ? v / 12.92 : ((v + .055) / 1.055) ** 2.4);
  return .2126 * c[0] + .7152 * c[1] + .0722 * c[2];
}
test("les couleurs essentielles conservent un contraste de texte de 4,5 dans les deux thèmes", () => {
  for (const theme of ["dark", "light"]) {
    const v = couleurs(theme);
    for (const [texte, fond] of [["--text", "--bg"], ["--muted", "--panel"], ["--accent2", "--panel"], ["--on-accent", "--accent"], ["--white", "--danger-bg"]]) {
      const [a, b] = [luminance(v(texte)), luminance(v(fond))].sort((a, b) => b - a);
      const ratio = (a + .05) / (b + .05);
      assert.ok(ratio >= 4.5, `${theme} ${texte}/${fond} : ${ratio.toFixed(2)}`);
    }
  }
});

test("une recherche ancienne se termine avant la vérification du logiciel suivant", async () => {
  const { serialiserResidus } = require(fileURLToPath(new URL("sessionResidus.cjs", sortie)));
  let terminer;
  const attente = new Promise((resolve) => { terminer = resolve; });
  const ordre = [];
  const ancienne = serialiserResidus(async () => { ordre.push("analyse"); await attente; ordre.push("fin analyse"); });
  const nouvelle = serialiserResidus(async () => { ordre.push("vérification"); });
  try {
    await Promise.resolve();
    assert.deepEqual(ordre, ["analyse"]);
  } finally { terminer(); await Promise.all([ancienne, nouvelle]); }
  assert.deepEqual(ordre, ["analyse", "fin analyse", "vérification"]);
  const resultats = await Promise.allSettled([serialiserResidus(async () => { throw new Error("échec attendu"); }), serialiserResidus(async () => 42)]);
  assert.equal(resultats[1].value, 42);
});

test("les noms restent lisibles sur les couleurs de la carte disque", async () => {
  const { colorFor } = require(fileURLToPath(new URL("Treemap.cjs", sortie)));
  const css = await readFile(new URL("../src/styles.css", import.meta.url), "utf8");
  const regle = css.match(/\.tile\s*\{([^}]+)\}/)[1];
  const tokenTexte = regle.match(/(?:^|;)\s*color:\s*var\((--[\w-]+)\)/)?.[1] ?? "--text";
  const hexHsl = (valeur) => {
    const [, h0, s0, l0] = valeur.match(/hsl\((\d+) (\d+)% (\d+)%\)/);
    const h = Number(h0), s = Number(s0) / 100, l = Number(l0) / 100;
    const a = s * Math.min(l, 1 - l);
    const canal = (n) => { const k = (n + h / 30) % 12; return Math.round(255 * (l - a * Math.max(-1, Math.min(k - 3, 9 - k, 1)))); };
    return "#" + [canal(0), canal(8), canal(4)].map((v) => v.toString(16).padStart(2, "0")).join("");
  };
  for (const theme of ["light", "dark"]) for (const kind of ["file", "dir", "small"]) for (let i = 0; i < 360; i++) {
    const fond = hexHsl(colorFor({ name: `Dossier ${i}`, path: `C:\\Fixture\\${i}`, kind, size: 1, files: 1, age_days: null }));
    const [a, b] = [luminance(couleurs(theme)(tokenTexte)), luminance(fond)].sort((a, b) => b - a);
    assert.ok((a + .05) / (b + .05) >= 4.5, `${theme}, ${kind}, ${fond} : contraste insuffisant`);
  }
});
