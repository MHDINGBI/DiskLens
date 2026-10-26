# DiskLens — guide d'utilisation

DiskLens est un utilitaire Windows local, en français, pour examiner l'espace disque, choisir ce qu'on nettoie et vérifier la santé du PC. Aucune action n'est exécutée sans votre confirmation.

## Télécharger et installer

Ouvrir <https://github.com/MHDINGBI/DiskLens/releases/latest>.

- **`DiskLens_0.2.0_x64-setup.exe`** : installateur Windows 64 bits. Double-cliquer et suivre l'assistant.
- **`DiskLens-Windows-x64.exe`** : version portable, sans installation. Double-cliquer directement. Microsoft Edge WebView2 doit être installé.
- **`SHA256SUMS.txt`** : empreintes des deux fichiers. Comparer l'empreinte du fichier téléchargé pour vérifier son intégrité.

Les exécutables ne sont pas signés numériquement. Windows peut afficher un avertissement SmartScreen. Vérifier la provenance du téléchargement avant exécution. DiskLens est expérimental : sauvegarder ses données avant tout nettoyage.

## Lancer l'application

Double-cliquer sur l'installateur ou le portable. Une fenêtre s'ouvre :

- **Accueil** : tâches courantes et espace réellement disponible sur les disques.
- **Stockage** : explorateur de disque, treemap, Top 20 des dossiers les plus volumineux et filtres.
- **Applications → Logiciels** : inventaire des programmes installés, recherche par nom ou éditeur, désinstallation officielle et recherche des restes.
- **Applications → Résidus** : fichiers et dossiers résiduels, registre avancé et quarantaine.
- **Suivi** : rapport mensuel, santé du disque et processus.
- **Outils** : OneDrive, connexions, extensions, sauvegarde et réglages.

## Nettoyer en toute sécurité

1. Ouvrir la rubrique concernée (par exemple **Applications → Logiciels**).
2. Sélectionner explicitement l'élément à traiter.
3. Lire l'avertissement s'affichant, puis confirmer.
4. DiskLens indique ce qui sera déplacé en quarantaine ou supprimé, jamais les deux sans confirmation.

La corbeille libère l'espace seulement après son vidage. Une mise en quarantaine sur le même disque ne libère pas d'espace avant la purge du lot. Les pistes incertaines restent en lecture seule et ne sont jamais ajoutées au panier de nettoyage.

## Ligne de commande

Ces commandes ne créent aucune fenêtre et ne nettoient rien :

```powershell
DiskLens.exe --help
DiskLens.exe --scan 'C:\Mon dossier' --json
DiskLens.exe --rapport-mensuel
DiskLens.exe --rapport-mensuel --data-dir 'C:\Mes rapports DiskLens'
```

- `--scan <chemin absolu> --json` : analyse en lecture seule d'un dossier, résultat JSON.
- `--rapport-mensuel [--data-dir <dossier>]` : génère le rapport local dans le dossier de données DiskLens (`%APPDATA%\com.omd.disklens`) ou le dossier indiqué.
- Un échec retourne un code non nul.

## Rapport mensuel

Dans **Rapport**, cliquer sur **Générer maintenant**. Le rapport est local, sans IA, sans e-mail automatique. Il réutilise les derniers scans et signale les données anciennes ou absentes.

La planification est désactivée par défaut. Si elle est activée explicitement, la tâche `DiskLens-RapportMensuel` s'exécute le premier de chaque mois à 09:00 dans la session Windows ouverte. Elle se désactive depuis le même écran.

Les exports se trouvent dans `rapports_mensuels`, sous le dossier de données de DiskLens.

## Mettre à jour

Fermer puis rouvrir DiskLens après chaque mise à jour. Télécharger la nouvelle version depuis les Releases. Vérifier l'empreinte SHA-256 avant d'exécuter le fichier.

## Limitations importantes

- Aucun fournisseur d'IA n'est appelé par les analyses, la CLI ou le rapport mensuel.
- Le repérage des connexions et extensions ne remplace pas un antivirus.
- Firefox et IPv6 sont hors de ce diagnostic.
- Les tests de compilation et de logique ne remplacent pas une validation visuelle complète de Windows.
- Certaines opérations nécessitent les droits administrateur et peuvent être refusées ; DiskLens n'élève pas ses droits silencieusement.
