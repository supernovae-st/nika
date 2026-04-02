# Agent Prompt — OPS Scaleway : Deploy nika serve + Wire Jungo

> **Copie-colle ce fichier entier comme premier message a un agent Claude Code.**
> Working directory: `/Users/thibaut/dev/supernovae/nika`

---

## Situation

On a 3 serveurs sur Scaleway. Le binaire nika sur le VPS date de v0.51 (on est a v0.58). Un agent parallele est en train de fixer 11 bugs dans nika serve — quand il finira on aura un binaire v0.58.1 propre a deployer. En attendant, on prepare tout.

### Infra actuelle

| Instance | Type | IP | Zone | Role | Status |
|----------|------|----|------|------|--------|
| **nk-dev-vps** | PLAY2-NANO | 51.15.136.200 | fr-par-1 | Thibaut dev/test | v0.51, daemon running, systemd Restart=always |
| **nk-jungo-vps** | (pas cree) | - | fr-par-1 | Nicolas prod, nika serve :3000 | A CREER |
| **nk-h100-gpu** | H100-1-80G | 51.159.153.241 | fr-par-2 | vLLM Qwen3.5-27B :8000 | UP, gere par Nicolas |

### Architecture cible

```
Nicolas (Node.js/Jungo)
  │  axios.post / axios.get
  │  Authorization: Bearer <token>
  ▼
nk-jungo-vps (:3000)              ← nika serve (Axum, embedded executor)
  │  ├── SQLite job queue
  │  ├── Rate limiting (10 req/s/token)
  │  ├── HMAC webhooks
  │  └── Prometheus /metrics
  │
  ├──[cloud]──> OpenAI / xAI / Gemini / Anthropic APIs
  │
  └──[private]──> nk-h100-gpu (:8000)
                  vLLM Qwen3.5-27B (custom endpoint)
```

### Ce que Nicolas a besoin

3 choses :
1. **URL** : `http://nk-jungo-vps:3000` (ou IP publique)
2. **Token** : string 32+ chars (on lui genere)
3. **Doc** : POST /v1/run + poll GET /v1/status — c'est tout

Nicolas fait du Node.js/TypeScript. Il ne sait pas ce que c'est Rust, vLLM, ou un daemon. Il fait POST, poll GET, recoit le resultat.

---

## Objectifs (par ordre)

### 1. Mettre a jour nk-dev-vps (v0.51 → v0.58.1)

Le VPS de dev a un binaire ancient. Il faut deployer le nouveau.

**Pre-requis :** L'agent Rust doit avoir fini les bug fixes et `cargo build --release` dans `tools/`. Verifier :
```bash
ls -la tools/target/release/nika
tools/target/release/nika --version
# Doit afficher v0.58.1 ou plus
```

**Deploy :**
```bash
# Depuis le Mac
scp tools/target/release/nika root@51.15.136.200:/tmp/nika-new

# Sur le VPS (SSH)
ssh root@51.15.136.200
systemctl --user stop nika-daemon
cp /tmp/nika-new ~/.nika/bin/nika
chmod +x ~/.nika/bin/nika
~/.nika/bin/nika --version  # verifier
systemctl --user start nika-daemon
systemctl --user status nika-daemon  # verifier running
```

**NOTE :** On ne peut PAS cross-compiler depuis macOS ARM vers Linux x64 facilement. Options :
- Option A : `cargo build --release` directement sur le VPS (lent, ~10 min sur PLAY2-NANO)
- Option B : Utiliser le binaire du GitHub Release CI (si on tag + push)
- Option C : `cross build --target x86_64-unknown-linux-gnu` (si cross est installe)

**Verifier apres deploy :**
```bash
# Sur le VPS
~/.nika/bin/nika --version       # v0.58.1
~/.nika/bin/nika provider list   # API keys OK
~/.nika/bin/nika doctor          # system health
```

### 2. Tester nika serve sur nk-dev-vps

Avant de creer le VPS de Nicolas, valider que serve marche sur le VPS de dev.

```bash
# Sur le VPS
export NIKA_SERVE_TOKEN=$(openssl rand -hex 32)
export NIKA_SERVE_BIND=0.0.0.0:3000
export NIKA_SERVE_WORKFLOWS=/opt/nika/workflows
mkdir -p /opt/nika/workflows

# Creer un workflow de test
cat > /opt/nika/workflows/test-hello.nika.yaml << 'EOF'
schema: "nika/workflow@0.12"
workflow: test-hello
description: "Simple hello for E2E test"
provider: openai
model: gpt-4.1-mini

inputs:
  topic: "AI"

tasks:
  - id: greet
    infer:
      prompt: |
        Say hello and give one fact about: {{inputs.topic}}
        Keep it under 50 words.
      temperature: 0.7
      max_tokens: 200
EOF

# Lancer serve
~/.nika/bin/nika serve
```

**Tester depuis le Mac :**
```bash
# Health
curl -s http://51.15.136.200:3000/health | jq .

# Run
curl -s -X POST http://51.15.136.200:3000/v1/run \
  -H "Authorization: Bearer <TOKEN>" \
  -H "Content-Type: application/json" \
  -d '{"workflow": "test-hello.nika.yaml", "inputs": {"topic": "AI"}}' | jq .

# Poll (avec le job_id retourne)
curl -s http://51.15.136.200:3000/v1/status/<JOB_ID> \
  -H "Authorization: Bearer <TOKEN>" | jq .
```

**Criteres de succes :**
- Health retourne `{"status":"ok","version":"0.58.1"}`
- POST /v1/run retourne un job_id
- GET /v1/status retourne "completed" avec un output PROPRE (texte seul, pas de CLI display)

### 3. Creer nk-jungo-vps sur Scaleway

**Console Scaleway :**
- Type : PLAY2-NANO (le meme que nk-dev-vps, ~€7/mois)
- Zone : fr-par-1
- Nom : nk-jungo-vps
- Image : Ubuntu 24.04
- Security group : `nika-sg` (SSH 22, HTTP 3000, Prometheus 9090)
- SSH key : la cle de Thibaut (deja dans le compte)

**Setup initial (SSH) :**
```bash
ssh root@<NEW_IP>

# Creer user nika
useradd -m -s /bin/bash nika
loginctl enable-linger nika

# Installer le binaire
mkdir -p /home/nika/.nika/bin
cp /tmp/nika-new /home/nika/.nika/bin/nika
chmod +x /home/nika/.nika/bin/nika
chown -R nika:nika /home/nika/.nika

# Ajouter au PATH
echo 'export PATH="$HOME/.nika/bin:$PATH"' >> /home/nika/.bashrc

# Setup les API keys
su - nika
nika provider set openai   # coller la cle
nika provider set xai      # coller la cle
nika provider list          # verifier

# Setup le daemon systemd
mkdir -p ~/.config/systemd/user
cat > ~/.config/systemd/user/nika-daemon.service << 'EOF'
[Unit]
Description=Nika Daemon
After=network.target

[Service]
Type=notify
ExecStart=%h/.nika/bin/nika daemon start --foreground
Restart=always
RestartSec=5
Environment=NIKA_NO_DAEMON=0

[Install]
WantedBy=default.target
EOF

systemctl --user daemon-reload
systemctl --user enable nika-daemon
systemctl --user start nika-daemon
systemctl --user status nika-daemon

# Setup nika serve comme service
cat > ~/.config/systemd/user/nika-serve.service << 'EOF'
[Unit]
Description=Nika HTTP API Server
After=nika-daemon.service
Requires=nika-daemon.service

[Service]
Type=simple
ExecStart=%h/.nika/bin/nika serve
Restart=always
RestartSec=5
EnvironmentFile=%h/.nika/.env

[Install]
WantedBy=default.target
EOF

# Creer le fichier env
cat > ~/.nika/.env << 'EOF'
NIKA_SERVE_TOKEN=<GENERER UN TOKEN 64 CHARS>
NIKA_SERVE_BIND=0.0.0.0:3000
NIKA_SERVE_WORKFLOWS=/opt/nika/nk-jungo/workflows
NIKA_SERVE_MAX_CONCURRENT=6
NIKA_SERVE_TIMEOUT=600
EOF
chmod 600 ~/.nika/.env

systemctl --user daemon-reload
systemctl --user enable nika-serve
```

### 4. Deployer nk-jungo (le vrai projet traduction)

```bash
# Sur nk-jungo-vps, en tant que user nika
sudo mkdir -p /opt/nika
sudo chown nika:nika /opt/nika
cd /opt/nika
git clone https://github.com/SuperNovae-studio/nk-jungo.git
# ou: git clone git@github.com:SuperNovae-studio/nk-jungo.git

# Verifier les workflows
nika check /opt/nika/nk-jungo/workflows/translate.nika.yaml
nika check /opt/nika/nk-jungo/workflows/translate-all.nika.yaml
nika check /opt/nika/nk-jungo/workflows/pull-repo.nika.yaml
nika check /opt/nika/nk-jungo/workflows/push-output.nika.yaml

# Demarrer serve
systemctl --user start nika-serve
systemctl --user status nika-serve
```

### 5. Wire vLLM (custom endpoint)

Le H100 a vLLM sur 51.159.153.241:8000. Configurer Nika pour l'utiliser :

```bash
# Sur nk-jungo-vps
echo 'NIKA_ENDPOINT_QWEN=http://51.159.153.241:8000/v1' >> ~/.nika/.env
systemctl --user restart nika-serve
```

Dans les workflows de traduction, utiliser :
```yaml
provider: openai
model: Qwen3.5-27B
base_url: "http://51.159.153.241:8000/v1"
```

Ou via l'env var : le SSRF auto-allow (BUG fix v0.55) autorise les custom endpoints.

### 6. Donner acces a Nicolas

Envoyer a Nicolas :
```
URL:   http://<nk-jungo-vps-ip>:3000
Token: <le token genere>

Exemple Node.js:
  const res = await axios.post('http://<ip>:3000/v1/run', {
    workflow: 'translate.nika.yaml',
    inputs: { file: 'ui.json', locales: ['fr-FR'] }
  }, { headers: { Authorization: 'Bearer <token>' } });

  // Poll toutes les 2s
  const status = await axios.get(`http://<ip>:3000/v1/status/${res.data.job_id}`, ...);
```

Le doc complet est dans `test-jungo/docs/ONBOARDING-NICOLAS.md`.

---

## Securite

- Token : 64 chars random (`openssl rand -hex 32`)
- `.env` : chmod 600, jamais dans git
- Security group : SSH 22 + HTTP 3000 seulement (pas de 80/443 pour l'instant)
- SSRF : custom endpoints auto-allowed, private IPs blocked sauf config explicite
- Le daemon gere les secrets vault (API keys chiffrees, pas en clair dans .env)
- Les API keys LLM sont dans le NikaVault, pas dans l'env du subprocess (apres BUG-2 fix)

## Monitoring

```bash
# Sur le Mac, pointer Prometheus vers le VPS
# nk-jungo-vps:3000/metrics expose les metriques

# Metriques disponibles :
# nika_jobs_total{status="completed|failed"}
# nika_jobs_active
# nika_job_duration_seconds
# nika_http_requests_total{method,path,status}
```

## Budget

- 2x PLAY2-NANO : ~€14/mois
- 1x H100 : ~€2000/mois (Nicolas gere)
- Cloud APIs : variable (~$50-100/mois pour les traductions)

## Blockers

| Blocker | Qui | Status |
|---------|-----|--------|
| BUG-1 fix (embedded executor) | Agent Rust | En cours |
| cargo build --release | Agent Rust | Apres les fixes |
| nk-jungo workflows finaux | A faire (HANDOFF.md dans nk-jungo) |
| Repo GitHub nk-jungo | A creer |
| Token pour Nicolas | A generer |
| DNS/domaine | Optionnel (IP directe suffit pour maintenant) |

## Fichiers de reference

| Fichier | Contenu |
|---------|---------|
| `supernovae/nk-jungo/HANDOFF.md` | Spec complete des 4 workflows i18n |
| `supernovae/nk-jungo/README.md` | Structure du projet nk-jungo |
| `test-jungo/docs/ONBOARDING-NICOLAS.md` | Guide pour Nicolas |
| `supernovae/docs/05-operations/stack/scaleway/` | Notes infra precedentes |
| `nika/docs/plans/2026-04-01-serve-bug-report.md` | Bugs nika serve (pour verifier les fixes) |
