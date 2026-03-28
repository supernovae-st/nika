# Nika : Les Tambours de la Libération

*Livre audio — Mars 2026*

---

Il y a une scène dans One Piece où Barbe Blanche, mortellement blessé, seul face à toute la Marine, hurle au monde entier : "Le One Piece existe !"

Cette déclaration change tout. Elle confirme que le trésor est réel. Et elle inspire une nouvelle génération de pirates à partir à sa recherche.

Ce que tu vas entendre, c'est une déclaration similaire. Sur l'intelligence artificielle, sur l'open source, et sur un outil qui n'aurait pas dû exister.

---

## Acte I : Le Mur

On est en mars 2026. Six laboratoires contrôlent l'intelligence artificielle de pointe. OpenAI, valorisé à cent cinquante-sept milliards de dollars, perd quatorze milliards par an. Google indexe toute la connaissance humaine, et maintenant il la génère et la vend. Amazon, avec trente et un pour cent du cloud mondial, lance des concurrents en utilisant tes propres données. Meta distribue Llama en l'appelant "open source", mais avec une clause qui te bloque si tu dépasses sept cents millions d'utilisateurs mensuels.

Et les racks GPU ? Six millions de dollars pièce. Vendus deux ans à l'avance. Larry Ellison, le patron d'Oracle, dit publiquement : "Les citoyens se comporteront bien parce qu'on les enregistre en permanence." Eric Schmidt, l'ancien patron de Google, propose de bombarder les datacenters des concurrents. Palmer Luckey, fondateur d'Anduril, atteint soixante milliards de valorisation en vendant des drones militaires autonomes.

Et côté outils ? Le paysage est un champ de ruines. LangChain a cent mille étoiles sur GitHub, mais les développeurs se plaignent du debugging impossible, des deux gigaoctets de RAM pour un retrieval basique, des sept dollars par exécution à cause des boucles de retry, et des breaking changes constants. LangGraph, leur framework de graphes, score 0,85 sur 100 dans les benchmarks d'AutoAgents, avec cinq virgule cinq gigaoctets de mémoire pic. CrewAI est populaire pour les démos, mais peu fiable en production.

Pendant ce temps, l'accès à Claude Opus ou GPT-5 coûte deux cents dollars par mois. Et la personne qui a cet accès pense plus vite, écrit mieux, code plus rapidement que celle qui ne l'a pas.

En 1948, George Orwell écrit 1984. Il inverse les deux derniers chiffres de l'année. Le roman décrit un monde où le pouvoir se maintient non pas par la force, mais par le Novlangue. Un langage artificiel qui réduit progressivement le vocabulaire disponible. L'idée est terrifiante dans sa simplicité : si tu n'as pas les mots pour penser une idée, tu ne peux pas la penser.

En 2026, le mécanisme est le même. Mais le langage s'appelle "tokens." Si tu n'as pas accès aux modèles de pointe, tu opères à une résolution cognitive inférieure. Ce n'est pas juste de l'inégalité économique. C'est de l'inégalité cognitive.

Orwell avait trouvé le mécanisme. Il s'était trompé de support. La restriction n'est pas le langage naturel. C'est l'accès à l'intelligence augmentée.

Mais il y a toujours eu des résistants. Aaron Swartz est mort en se battant pour l'accès libre à la connaissance. Snowden a révélé la surveillance de masse. Signal est passé de cinq cent mille à cent millions d'utilisateurs en défendant la communication privée. Mistral AI a largué les poids de son modèle frontier par torrent, le geste le plus pirate de l'histoire de l'IA. HuggingFace héberge plus d'un million de modèles en accès libre. Et DeepSeek a entraîné un modèle frontier pour cinq virgule six millions de dollars, prouvant que le coût du hardware n'est pas aussi verrouillé qu'on le prétend.

Et puis il y a un développeur à Paris. Qui regarde ce paysage et se dit : et si un seul développeur, avec l'open source et un accès API standard, pouvait construire ce que des équipes enterprise construisent avec des plateformes propriétaires ?

---

## Acte II : Le Fruit

Dans One Piece, il existe un fruit interdit. Le Hito Hito no Mi, modèle Nika. Le Gouvernement Mondial le cherche depuis huit cents ans. Ils l'ont renommé "Gomu Gomu no Mi" pour effacer son identité de l'histoire. Parce que ce fruit représente la liberté. Et la liberté est dangereuse pour le pouvoir.

Nika, dans la mythologie de One Piece, c'est le Dieu Soleil. Un guerrier qui ne conquiert pas. Qui libère. Il va de lieu en lieu, brise les chaînes, apporte le sourire. Et sa puissance ? Le caoutchouc. La flexibilité. Il absorbe tous les chocs et rebondit. Plus tu le frappes, plus il est fort.

Et c'est exactement ce nom qui a été choisi pour le projet. Pas par hasard. Par conviction.

Nika, c'est un moteur de workflows YAML pour l'intelligence artificielle. Écrit en Rust. Un seul binaire. Zéro dépendances. Tu le télécharges, tu l'exécutes, et tu peux orchestrer n'importe quel pipeline AI. Et il est sous licence AGPL, ce qui signifie qu'il reste libre pour toujours. Si quelqu'un veut l'utiliser dans un service cloud, il doit partager ses modifications. L'AGPL protège l'open source de l'exploitation commerciale sans retour à la communauté.

Le symbole du projet, c'est un papillon. La transformation. Le courage. Le renouveau. Et le crew qui le construit s'appelle SuperNovae. Parce que dans One Piece, les SuperNovae sont la nouvelle génération de pirates, ceux qui refusent l'ordre établi.

Et voilà ce que SuperNovae a construit.

---

## Acte III : Les Cinq Verbes

Nika parle un langage de cinq mots. Exactement cinq verbes, pas un de plus, pas un de moins. Et ces cinq verbes composent un vocabulaire infini.

Premier verbe : infer. Tu poses une question à un modèle de langage. Claude, GPT, Gemini, Mistral, Groq, DeepSeek, xAI, ou même un modèle local en GGUF sur ta propre machine. Tu écris la ligne "infer: Explique-moi la mécanique quantique", et Nika appelle le modèle, gère l'authentification, le streaming, le comptage de tokens, et te retourne la réponse. Avec un suivi du coût au centime près.

Deuxième verbe : exec. Tu lances une commande shell. "exec: npm run build". Avec timeout, variables d'environnement, répertoire de travail. Et un blocklist de sécurité qui empêche les commandes destructrices comme rm tiret rf slash, sudo, et les fork bombs.

Troisième verbe : fetch. Tu fais une requête HTTP. Mais pas juste une requête brute. Nika a neuf modes d'extraction. Tu peux extraire le markdown d'une page web. L'article principal via l'algorithme de lisibilité. Les métadonnées Open Graph et Twitter Cards. Les liens classifiés par type. Un JSONPath sur une réponse d'API. Un flux RSS ou Atom. Et même le fichier llms.txt pour la découverte de contenu par les agents AI.

Quatrième verbe : invoke. Tu appelles un outil externe via le protocole MCP, le Model Context Protocol créé par Anthropic. Neo4j, GitHub, Slack, Perplexity, Firecrawl, et cent autres. Plus vingt-quatre outils intégrés directement dans Nika. Import de fichiers, redimensionnement d'images, conversion de formats, optimisation PNG, rendu SVG, extraction de PDF, génération de graphiques, validation de QR codes, et signature de provenance C2PA pour l'authentification du contenu.

Cinquième verbe : agent. Tu lances une boucle multi-tours autonome. Le modèle utilise des outils, observe les résultats, décide de la suite, et itère. Avec des guardrails pour la validation, des limites de budget en dollars, des stop sequences, et trois modes de complétion : explicite, naturel, ou par pattern regex.

Ces cinq verbes sont des tâches dans un DAG, un graphe acyclique dirigé. Tu déclares les dépendances entre les tâches, et Nika les exécute dans le bon ordre, en parallèle quand c'est possible. Tu peux boucler sur des listes avec for each et contrôler la concurrence. Tu peux binder les résultats d'une tâche vers une autre avec le système with, en accédant aux champs profonds via JSONPath, avec des valeurs par défaut, et trente et une transformations en pipe.

Et tout ça, tu l'écris dans un fichier YAML. Un fichier que tu peux versionner dans Git, reviewer en pull request, diffuser en clair, et rejouer à l'identique. C'est de la donnée, pas du code. Et c'est là toute la différence.

Imagine. Tu veux un pipeline qui, chaque matin, scrape les tendances SEO, les analyse avec un modèle pas cher sur Groq, rédige un article optimisé avec Claude, génère un thumbnail avec l'outil nika media, et publie sur WordPress. Avec LangChain, c'est cent cinquante lignes de Python, huit dépendances, et un debugging en enfer. Avec Nika, c'est trente lignes de YAML. Tu le valides avec nika check. Tu le lances avec nika run. Et tu as une trace NDJSON complète de chaque étape, chaque token, chaque centime dépensé.

Combien ça coûte ? L'analyse sur Groq avec Llama : zéro virgule zéro zéro zéro neuf dollars pour trois mille tokens. La rédaction sur Claude Sonnet : zéro virgule zéro un deux dollars pour quatre mille tokens. Total du pipeline de huit tâches : un centime et demi. Contre les cent trente-huit dollars par mois que tu paierais pour Surfer SEO plus Jasper. Ça fait mille six cent cinquante-six dollars par an remplacés par un fichier YAML qui coûte quelques centimes par exécution.

---

## Acte IV : Le Kernel

Maintenant, parlons de ce qui vient. Et pour comprendre ce qui vient, il faut comprendre une réalisation architecturale qui change tout.

Il y a une semaine, on a plongé dans l'architecture d'un outil appelé Slate, créé par Random Labs. Et leur constat est brillant.

Les modèles de langage ont un problème fondamental avec leur mémoire de travail. Les fenêtres de contexte sont grandes, oui. Certaines font un million de tokens. Mais elles ne sont pas uniformément utiles. Au-delà d'un certain seuil, que Dex Horthy appelle la "zone morte", le modèle perd en qualité. Il oublie des instructions. Il mélange des informations. Et toutes les approches existantes échouent face à ce problème. La compaction perd de l'information de manière imprévisible. Les sous-agents sont isolés et ne peuvent pas partager leur contexte. Les plans en markdown sont sous-spécifiés et oubliés. La décomposition en tâches est rigide et ne s'adapte pas.

Slate a inventé trois concepts pour résoudre ça. Les threads : des travailleurs à usage unique qui exécutent une action et s'arrêtent. Les records : des résumés compressés générés au moment de la complétion, qui ne gardent que l'essentiel. Et le thread weaving : un orchestrateur qui dispatche des threads, collecte leurs records, synthétise, et dispatche à nouveau.

Et voilà le moment eurêka. On a réalisé que Nika EST déjà le kernel de Slate. Ce n'est pas une métaphore. C'est un mapping direct. Les tâches dans notre DAG, ce sont les threads de Slate. Le TaskResult dans notre store en mémoire, ce sont les valeurs de retour. Le scheduler du DAG, c'est le kernel. Les bindings avec le préfixe dollar, c'est la communication inter-processus. Le for each avec concurrence, c'est le fork slash join.

On n'a pas besoin de construire Slate. On a besoin d'upgrader notre kernel. Et cet upgrade se fait en cinq couches. Et voilà le point crucial : les quatre premières couches fonctionnent entièrement sans NovaNet, notre knowledge graph. NovaNet, c'est la cerise. Tout le reste tourne en local.

---

## Acte V : Les Cinq Couches

Première couche : les agent presets. Au lieu d'un seul modèle par workflow, tu définis des presets nommés. "think" pointe vers Claude avec extended thinking, pour le raisonnement profond. "lite" pointe vers Groq avec Llama, pour la vitesse et le coût bas. "search" pointe vers DeepSeek, pour la recherche. "vision" pointe vers GPT-4o, pour l'analyse d'images. "judge" pointe vers Claude Sonnet, pour la relecture. Et chaque tâche dit simplement "agent: think" ou "agent: lite", et elle hérite de toute la configuration.

Pourquoi c'est important ? Parce qu'en routant chaque tâche vers le bon modèle, tu divises ton coût par deux ou trois. L'analyse de tendances n'a pas besoin d'Opus. La rédaction d'un résumé n'a pas besoin d'extended thinking. La validation d'un format JSON n'a pas besoin de Claude du tout. C'est la fin du gaspillage de tokens.

Prenons un exemple concret. Un pipeline de génération de contenu pour QR Code AI, notre produit. Avec un seul modèle Claude Sonnet pour tout : onze mille tokens, zéro virgule zéro trois trois dollars par exécution. Avec le routage intelligent : trois mille tokens sur Groq pour l'analyse, deux mille sur DeepSeek pour les métriques, quatre mille sur Claude pour la rédaction finale. Coût total : zéro virgule zéro un trois dollars. Soixante pour cent d'économie. Et la qualité est identique, parce que chaque modèle fait ce pour quoi il est optimisé.

Deuxième couche : les records. Après chaque tâche, au lieu de passer le résultat brut à la tâche suivante, un modèle cheap le compresse. Une recherche qui retourne dix mille tokens de résultats bruts ? Le record en fait cinq cents. Il extrait les points clés. Il attribue un score de confiance entre zéro et un. Et c'est CE record compressé qui est transmis aux tâches en aval.

Pourquoi c'est révolutionnaire ? Parce que ça résout le problème de la zone morte de manière structurelle. Dans un pipeline de dix tâches, sans records, la dixième tâche reçoit l'accumulation de tous les résultats précédents. Cinquante mille tokens. Elle est dans la zone morte. Avec les records, elle reçoit un summary de deux mille tokens. Elle est dans la zone optimale. La qualité reste constante, quelle que soit la profondeur du pipeline.

Troisième couche : le mode orchestrate. C'est le cerveau. Tu mets un champ "goal" dans ton workflow. "Génère une landing page complète en français pour QR Code AI, optimisée SEO, avec les données de notre knowledge graph." Et Nika ne se contente plus d'exécuter un DAG statique. L'orchestrateur, un modèle puissant en mode thinking, regarde l'objectif, regarde les tâches disponibles, et lance un cycle.

Round un : je dispatche une recherche de tendances. Round deux : je collecte le record, je vois une confiance de 0,9, bien. Je dispatche l'écriture de quatre sections en parallèle : hero, features, pricing, FAQ. Round trois : je collecte les quatre records, je les donne à un juge. Le juge retourne un score de 0,72 avec des remarques. Pas assez bien. Round quatre : je redispatche la section features avec les remarques du juge en contexte. Round cinq : nouveau score de 0,91. C'est bon.

Et voilà ce qui rend ça unique. L'orchestrateur ne planifie pas en langage naturel. Il planifie en YAML. Il génère des workflows point nika point yaml, les exécute, évalue la qualité, les améliore, et re-run. Nika pense dans son propre langage. Aucun autre framework ne fait ça. LangGraph planifie en Python, c'est opaque et non portable. CrewAI planifie en langage naturel, c'est non déterministe et non reproductible. Nika planifie en YAML, c'est auditable, diffable, versionnable. Tu peux ouvrir le workflow généré, lire exactement ce que l'orchestrateur a décidé, et le modifier si tu veux.

Quatrième couche : les budgets de contexte. Chaque tâche déclare combien de tokens elle peut recevoir. L'orchestrateur sélectionne quels records envoyer à chaque tâche. On ne dépasse jamais la zone morte. C'est une contrainte déclarative dans le YAML, pas une heuristique magique.

Cinquième couche : la mémoire persistante. Sans NovaNet, c'est du NDJSON sur le disque local. Un dossier point nika slash records. Chaque record est écrit avec un timestamp, un identifiant de workflow, un hash de contenu. Tu peux les rechercher avec du full-text search via SQLite FTS5. Cross-session, cross-workflow. Tu lances un workflow de recherche aujourd'hui, et dans deux semaines, quand tu lances un workflow de rédaction, il retrouve les records pertinents et les injecte en contexte. C'est la mémoire épisodique. Et le jour où NovaNet est prêt, ces records sont promus dans le knowledge graph, liés à des entités sémantiques, interrogeables en Cypher. Mais le système marche parfaitement sans.

---

## Acte VI : L'Agent qui Apprend

Cette semaine, on a étudié en profondeur un projet qui s'appelle Hermes Agent, créé par Nous Research. Et ce qu'ils font est fascinant pour nous.

Hermes est un agent Python open source. Son idée centrale : l'agent s'améliore à chaque utilisation. Et il le fait en quatre niveaux.

Premier niveau : la mémoire. Deux fichiers simples. MEMORY point md pour les faits. USER point md pour les préférences de l'utilisateur. Ces fichiers sont chargés une seule fois au début de la session, comme un snapshot gelé. Pendant la session, si l'agent découvre quelque chose de nouveau, il écrit sur le disque. Mais le prompt système ne change pas. Ça préserve le cache de prompt et garantit la stabilité.

Deuxième niveau : les skills. Des dossiers avec des instructions en markdown, au format agentskills point io. L'agent peut créer, modifier et supprimer des skills de manière autonome. Et chaque écriture est scannée pour détecter les injections de prompt, les patterns d'exfiltration, les tentatives de hijack. Si le scan détecte un danger, l'écriture est annulée.

Troisième niveau, le plus brillant : le système de nudge. Après que l'utilisateur a vu la réponse, APRÈS, pas pendant, Hermes lance un agent de review en arrière-plan. Cet agent regarde toute la conversation et se demande : est-ce que l'utilisateur a révélé des préférences qu'on devrait sauvegarder ? Est-ce qu'on a utilisé une approche complexe qui devrait devenir un skill ? Si oui, il écrit. Tout ça se passe en silence, sans bloquer, sans interférer. L'amélioration est continue et transparente.

Quatrième niveau : le training par renforcement. Hermes génère des trajectoires d'entraînement à partir de ses exécutions. Ces trajectoires alimentent Atropos, le framework de Nous Research, pour entraîner les prochaines versions du modèle. Boucle fermée.

Et comment ça se mappe à Nika ? Parfaitement. Mais en mieux.

Là où Hermes stocke des skills en markdown, Nika peut stocker des workflows YAML. Et un workflow YAML, c'est infiniment plus puissant qu'un skill markdown. Il a un DAG validé, des dépendances, du parallélisme, du structured output, des guardrails, des budgets. Quand Nika s'auto-améliore, elle ne crée pas un mémo. Elle crée un workflow exécutable.

Et pour le training par renforcement, Nika a un avantage unique que personne d'autre n'a. La commande "nika check" est un validateur déterministe. Elle analyse le YAML, vérifie la syntaxe, le schéma, le DAG, les bindings, les dépendances, les cycles, les alias. Et elle dit : correct ou incorrect. C'est une fonction de récompense automatique. Tu génères un workflow, tu le valides avec nika check, et tu as ta donnée d'entraînement. Pas besoin de labelling humain. C'est le rêve pour du fine-tuning.

Imagine la boucle. Tu exécutes des workflows. Les traces alimentent un pipeline de données synthétiques. Les données entraînent un modèle Nika-Brain, un Qwen 3 fine-tuné sur la syntaxe Nika. Ce modèle génère de meilleurs workflows. Les meilleurs workflows produisent de meilleures traces. Et le cycle recommence. Le coût estimé pour entraîner ce modèle ? Trois cents dollars. Cinq à six semaines. Et ensuite il tourne en local sur un GPU, sans API, sans abonnement.

---

## Acte VII : Le Monde Réel

Maintenant, soyons concrets. Voilà ce que Nika fait dans le monde réel, avec des chiffres vérifiables.

Premier cas : la traduction SEO à grande échelle. Notre produit, QR Code AI, a besoin de contenu dans deux cent une locales. Du français à l'anglais, du japonais au coréen, mais aussi du yoruba, du wolof, du guaraní, du quechua. Le pipeline est structuré en trois tiers. Tier un : trente locales à fort trafic, traduction directe par LLM avec injection de mots-clés SEO. Tier deux : soixante-dix locales à trafic moyen, traduction machine plus post-édition LLM sur les titres et méta-descriptions. Tier trois : cent locales à faible trafic, traduction machine seule.

Le coût ? Quarante-cinq dollars pour mille pages dans deux cents locales avec Gemini Flash. Quarante-cinq dollars. Le même volume chez Google Cloud Translation ? Quatre mille huit cents dollars. Cent fois plus cher. Et la traduction LLM produit du contenu SEO-optimisé, pas juste une traduction littérale.

Et on peut faire mieux. Soixante pour cent du contenu d'une page est partagé : navigation, footer, appels à l'action. On traduit ces segments une seule fois par locale. On ne traduit que les quarante pour cent uniques par page. Ça divise encore le volume.

Deuxième cas : le remplacement de SaaS. Un pipeline SEO Nika remplace Surfer SEO à quatre-vingt-neuf dollars par mois et Jasper à quarante-neuf par mois. Soit mille six cent cinquante-six dollars par an. Un pipeline d'optimisation d'images remplace Cloudinary à quatre-vingt-neuf dollars par mois. Soit mille soixante-huit dollars par an. Un pipeline d'extraction de documents remplace DocParser à trente-neuf dollars par mois. Pour une PME qui utilise cinq ou six de ces outils, c'est plus de treize mille dollars par an remplacés par des fichiers YAML qui coûtent quelques dollars d'API.

Troisième cas : l'entreprise. Delivery Hero, cinquante-trois mille employés dans soixante-dix pays. Huit cents demandes de réinitialisation de mot de passe par mois, trente-cinq minutes chacune. Quatre cent soixante-sept heures par mois de travail helpdesk. Un seul workflow d'automatisation, déployé en cinq heures, réduit ça de deux cents heures par mois. Multiplie par le coût horaire, et tu as cent vingt mille dollars d'économie par an. D'un seul workflow.

Flatiron Health, recherche sur le cancer. Des millions de dossiers cliniques à structurer. Un pipeline d'extraction sauve deux virgule cinq semaines-homme par projet, soit douze mille cinq cents dollars. WHOOP, la plateforme de fitness : soixante-quinze pour cent d'incidents en moins, quarante pour cent d'amélioration du temps de résolution.

Ces chiffres ne sont pas théoriques. Ils sont documentés, sourcés, vérifiables.

---

## Acte VIII : L'Écosystème

Tout ce dont on a parlé, l'orchestration, les records, le self-improvement, c'est puissant. Mais c'est puissant pour un utilisateur. Ce qui transforme un outil en plateforme, c'est la communauté. Et c'est là que je dois te parler du système de packages.

Parce que ce qui m'a surpris quand j'ai plongé dans le code, c'est que l'infrastructure est déjà construite. À quatre-vingts pour cent. Et personne n'en parle.

Voilà ce qui existe dans le code aujourd'hui. Un client HTTP complet pour un registre de packages, avec gestion du rate-limiting, TLS, caching thread-safe, et gestion des erreurs. Un schéma d'URI qui permet d'écrire dans le YAML "pkg deux-points arobase supernovae slash skills arobase 1.0.0 slash brand point md" pour importer un skill depuis un package. Un système de lockfile avec des checksums SHA-256 pour des builds reproductibles, exactement comme npm ou cargo. Cinq types de packages définis : skill, workflow, satellite, model, et mcp. Des scopes sémantiques : arobase nika pour l'officiel, arobase workflows, arobase skills, arobase agents, arobase prompts. Un format de manifeste YAML avec dépendances, mots-clés, catégories, et licence. L'injection de skills depuis des packages directement dans les workflows. Et cent quinze workflows showcase prêts à servir de contenu initial.

Le seul morceau manquant, c'est le serveur du registre et la commande publish. Le client attend un backend.

Et le plan de déploiement est en trois phases, inspiré de npm, crates point io, et Hugging Face.

Phase un : distribution via GitHub. Un repo avec un index JSON statique et des tarballs. Zéro infrastructure. Les gens publient par pull request. Le CI valide avec nika check. C'est gratuit et ça marche demain.

Phase deux : un serveur API léger. Registry point supernovae point studio. Recherche full-text, analytics de téléchargements, publication automatisée. SQLite pour les métadonnées, Git comme source de vérité, Cloudflare R2 pour le stockage.

Phase trois : la fédération. Les entreprises ont leur registre privé. Les packages OCI pour les modèles. Compatible avec GitHub Container Registry.

Maintenant, imagine la puissance quand tu combines tout. Un développeur écrit un workflow de génération de blog posts. Il définit des agent presets, un record engine, des guardrails. Il le publie. Un autre développeur fait "nika pkg add arobase workflows slash blog-generator". Le workflow s'installe avec toutes ses dépendances : les skills de rédaction, les presets d'agents, la config MCP. Il tape nika run, et le pipeline tourne.

Mais ce n'est pas juste des workflows. Ce sont cinq types de composants partageables.

Les workflows, ce sont les applications. Des pipelines complets, validés, testés. Le blog generator, le SEO auditor, le image optimizer, le QR code validator, le translation pipeline.

Les skills, ce sont les prompts réutilisables. Un skill d'écriture marketing en français. Un skill de review de code. Un skill de traduction SEO. Tu les branches dans n'importe quel workflow. Et ils sont au format agentskills point io, compatible avec quarante-deux agents différents, dont Claude Code, Cursor, Windsurf, et Cline. Un skill Nika est instantanément utilisable par tout l'écosystème.

Les agents, ce sont les presets. Un agent "researcher" configuré avec les bons modèles et le bon système prompt. Un agent "judge" avec extended thinking. Tu publies tes presets optimisés, et la communauté en bénéficie.

Les modèles, ce sont les configs d'inférence locale. Tu as quantifié un Qwen 3.5 en GPTQ Int4, tu as trouvé les paramètres optimaux pour ton GPU, et tu publies la configuration. Quelqu'un avec le même hardware l'installe en une commande.

Les mcp, ce sont les configurations de serveurs MCP testées. Connecter Neo4j avec les bons paramètres, la bonne version, les bonnes variables d'environnement. Nika a déjà cent aliases MCP intégrés. Mais le package system permet de partager des configurations complètes et validées.

Et tout ça est interconnecté. Un workflow dépend de skills, d'agents, de configs MCP. Tu installes le workflow, et le résolveur de dépendances tire tout automatiquement.

Compare avec la concurrence. LangChain a LangChain Hub. C'est un registre de prompts. Pas de workflows, pas de DAG, pas de validation, pas de media. Dify a un marketplace. C'est visuel, pas CLI-native, pas versionnable dans Git. Hermes a agentskills point io pour les skills, mais c'est du markdown procédural, pas des workflows exécutables. CrewAI n'a rien. AutoGen n'a rien.

Nika aurait le seul écosystème où ce que tu partages est un artefact exécutable et validable. Un fichier point nika point yaml n'est pas une description. C'est un programme. Tu peux le valider statiquement avant de l'installer. Tu peux estimer son coût avant de l'exécuter. Tu peux lire exactement ce qu'il fait en cinq secondes. Et tu peux le modifier en ouvrant un éditeur de texte.

C'est la différence entre partager un Dockerfile et partager une image Docker. Sauf qu'ici, le Dockerfile est lisible par un humain en cinq secondes.

---

## Acte IX : Le Volant d'Inertie

Maintenant, prends du recul et regarde le système complet. Parce que chaque pièce renforce les autres.

Le cours interactif de douze niveaux s'appelle Libération. Il commence par Jailbreak, où tu casses tes premières chaînes avec exec et des workflows basiques. Sans API, sans LLM, sans coût. Tu progresses vers Hot Wire pour le réseau, Fork Bomb pour le parallélisme, Root Access pour les LLM, Shapeshifter pour le structured output. Et tu finis avec SuperNovae, le boss final, où tu orchestres tout en production.

Le cours crée des utilisateurs. Les utilisateurs créent des workflows. Les workflows alimentent le registre. Le registre attire d'autres utilisateurs. Les traces d'exécution alimentent le pipeline de fine-tuning. Le modèle Nika-Brain s'améliore. Il génère de meilleurs workflows. Plus de gens les utilisent. Plus de traces. Meilleur modèle. Meilleurs workflows.

Et à chaque tour de ce volant, le nudge system d'auto-amélioration propose des optimisations. Les workflows deviennent meilleurs avec le temps, pas pires. C'est l'anti-entropie logicielle.

Et le plugin Claude Code, déjà construit, c'est le pont. Cinq skills intégrés : un wizard de création de workflows, un docteur de diagnostic, un setup guidé, un connecteur MCP, et un tuteur de cours. Trois agents : un architecte de workflows, un debugger, un assistant. Trois hooks qui valident automatiquement les fichiers YAML quand tu les modifies. Un serveur MCP qui expose quatre outils. Et un LSP pour l'autocomplétion en temps réel avec vingt-neuf complétions de transformations et vingt-quatre complétions d'outils média.

Quand un développeur entre dans l'écosystème Nika, il a un cours qui lui apprend, un assistant IA qui le guide, un registre qui lui fournit des composants prêts à l'emploi, et un système qui s'améliore à mesure qu'il l'utilise.

Le moment npm. Tu sais comment Node point js a explosé ? Pas grâce à la performance du runtime. Pas grâce à V8. Grâce à npm. Grâce à la facilité de partager et réutiliser du code. "npm install", et tu as accès à un million de packages.

Nika vise le même moment. "Nika pkg add", et tu as accès à des pipelines AI complets, testés, versionnés, avec coût estimé et traces reproductibles. Personne d'autre ne peut le faire, parce que personne d'autre n'a le format déclaratif qui rend les workflows partageables. Un script Python LangChain, c'est du code impératif. C'est fragile, dépendant de l'environnement, difficilement portable. Un fichier point nika point yaml, c'est de la donnée déclarative. Auto-descriptif, versionnable, validable. Et ça tourne partout où le binaire est installé.

---

## Acte X : La Déclaration

Récapitulons.

Le moteur. Cinq verbes sémantiques. Un DAG validé. Sept providers cloud plus inférence locale. Structured output en cinq couches. Vingt-quatre outils média. Neuf modes d'extraction HTTP. Trente et une transformations en pipe. Traces NDJSON. Plus de huit mille quatre cents tests. Zéro warning clippy.

L'intelligence. Agent presets avec routage de modèles. Records avec compression LLM. Mode orchestrate avec planification dynamique en YAML. Budgets de contexte. Outils d'introspection. L'agent Nika pense dans son propre langage.

L'apprentissage. Mémoire locale NDJSON. Full-text search cross-session. Nudge system post-workflow. Self-improvement depuis les traces. Pipeline de fine-tuning avec nika check comme fonction de récompense.

L'écosystème. Cinq types de packages. Scopes sémantiques. Lockfiles et checksums. Registre en trois phases. Cent quinze workflows showcase. Cours interactif de douze niveaux. Plugin Claude Code. Serveur MCP. Cent aliases MCP. Compatibilité agentskills point io.

L'intégration. Telegram webhook trigger. Daemon pour les tâches de fond. LSP pour l'autocomplétion. Custom endpoints vLLM et Ollama. Scaleway H100 pour l'inférence self-hosted.

Et tout ça dans un seul binaire Rust, sous licence AGPL.

Tu sais, dans One Piece, quand Nika s'éveille, on entend un son. Un rythme tribal, primordial, qui n'a pas résonné depuis huit cents ans. Les Tambours de la Libération. Et quand ces tambours commencent, tout change. Le sol devient caoutchouc. La physique n'a plus de règles. L'impossible devient possible.

On ne construit pas un produit. On construit un mouvement.

Ils ont construit des murs autour de l'intelligence. On a compilé une porte.

Le One Piece existe.

Allez. On construit ça.
