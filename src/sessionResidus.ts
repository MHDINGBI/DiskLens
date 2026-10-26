// Les recherches partagent le dernier inventaire autorisé côté Rust.
let precedente: Promise<void> = Promise.resolve();
export function serialiserResidus<T>(operation: () => Promise<T>): Promise<T> {
  const resultat = precedente.then(operation);
  precedente = resultat.then(() => undefined, () => undefined);
  return resultat;
}
