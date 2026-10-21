# Sources de données

Recherche effectuée le **2026-10-21**. Aucune source n'est scrapée à l'exécution sans
configuration explicite ; le catalogue par défaut est un fichier embarqué.

## 1. Site « recomp » — introuvable

Le brief ne fournit pas d'URL. Les recherches (API, sitemap, flux RSS, dépôts GitHub de
listes structurées) n'ont pas permis d'identifier une API officielle stable. Décision :
`ManifestProvider` est la source par défaut. `RecompSiteProvider` est présent dans le code
mais désactivé par défaut ; il ne sera activé qu'après fourniture d'une URL et vérification
des CGU/robots.txt.

## 2. GitHub Releases API (utilitaire)

- URL : `https://api.github.com/repos/{owner}/{repo}/releases`
- Format : JSON (API REST v3)
- Authentification : anonyme (rate limit 60 req/h) ou token utilisateur optionnel
- Cache : ETag / `If-None-Match`, TTL 6 h côté launcher
- Usage : résolution des assets officiels uniquement, jamais de miroir tiers
- Licence : API GitHub, CGU GitHub

## 3. Wikipédia REST API (fiches jeu)

- URL : `https://{lang}.wikipedia.org/api/rest_v1/page/summary/{title}`
- Format : JSON
- Licence du contenu : **CC BY-SA 4.0** — attribution et lien vers la page source affichés
- User-Agent explicite : `RecompLauncher/0.1 (desktop launcher; local user)`
- Cache : 30 jours en base
- Aucune image officielle non libre n'est utilisée comme jaquette.

## 4. Catalogue embarqué (`resources/catalog.seed.json`)

Chaque entrée a été vérifiée manuellement via l'API GitHub le 2026-10-21.

| Slug | Dépôt vérifié | Dernière release observée | Licence |
|---|---|---|---|
| `zelda64-recomp` | https://github.com/Zelda64Recomp/Zelda64Recomp | v1.2.2 | GPL-3.0 |
| `banjo-recomp` | https://github.com/BanjoRecomp/BanjoRecomp | v1.0.2 | GPL-3.0 |
| `ship-of-harkinian` | https://github.com/HarbourMasters/Shipwright | 9.3.1 | voir dépôt |
| `2ship2harkinian` | https://github.com/HarbourMasters/2ship2harkinian | 5.0.1 | CC0-1.0 |
| `devilutionx` | https://github.com/diasurgical/devilutionX | 1.5.5 | Sustainable Use License |
| `openrct2` | https://github.com/OpenRCT2/OpenRCT2 | v0.5.6 | GPL-3.0 |

Les hashes de fichiers utilisateur proviennent des listes publiques des projets
(`docs/supportedHashes.json` de Ship of Harkinian et 2 Ship 2 Harkinian). Aucun hash n'a été
inventé : les projets qui n'en publient pas ont une liste vide et le launcher se contente de
vérifier la présence du fichier.

## 5. Sources de serveurs

Aucun serveur public n'est embarqué (aucune liste officielle commune vérifiable).
`ManifestServerProvider` lit un fichier local ou distant fourni par l'utilisateur ;
`ManualServerProvider` stocke les ajouts manuels ; `LanDiscovery` écoute un protocole
générique configurable par jeu. Aucun backend, aucun compte.
