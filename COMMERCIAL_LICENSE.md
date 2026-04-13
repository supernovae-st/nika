# Commercial License for Nika

Nika is dual-licensed. You can use it under either:

1. **[AGPL-3.0-or-later](LICENSE)** — the default, free and open source
2. **Commercial license** — for organizations that cannot comply with AGPL

This document explains when you need a commercial license, how to obtain one, and why dual licensing works this way.

## TL;DR

- **You can probably use Nika under AGPL for free.** Most use cases do.
- **You need a commercial license** if you modify Nika and embed it in a proprietary product you distribute, or if your legal/procurement team refuses AGPL.
- **Contact:** commercial@supernovae.studio

## When AGPL is Enough (most users)

You can use Nika under AGPL-3.0-or-later, at no cost, for:

- **Running workflows in your own organization** — internal tools, automation, research, experimentation. AGPL imposes no obligation when you do not distribute the software.
- **Building AGPL-compatible products** that use Nika as a dependency. Your product inherits AGPL obligations (source must be available to users), which is fine if your entire stack is open source.
- **Hosting a public or private service** using `nika serve` — but under **AGPL Section 13**, users interacting with that service over a network must have access to the complete source code of the Nika version you are running, including your own modifications. If you are OK with that, you are fine.
- **Contributing improvements upstream.** Fork, patch, submit a PR. You remain the copyright owner of your contribution (see [CLA.md](CLA.md)).
- **Academic research and teaching.** No commercial license needed for non-commercial academic use.
- **Personal projects, side projects, startups still in development.** Ship when ready.

If any of the above describes your situation, use Nika under AGPL. No paperwork, no fees, no tracking.

## When You Need a Commercial License

A commercial license from SuperNovae is required when you want to use Nika in ways that AGPL does not permit, specifically:

### 1. Proprietary embedded use

You modify Nika (or integrate it with proprietary code in a way that creates a derivative work) and ship the result inside a closed-source product to customers. AGPL would require you to publish your modifications and any code combined with Nika; a commercial license lifts that requirement.

### 2. Proprietary SaaS modifications

You run a service that uses Nika internally, and you modify Nika to add proprietary features. Under AGPL Section 13, those modifications must be available to your users. If that is not acceptable to your business, a commercial license is available.

### 3. Enterprise procurement constraints

Your legal or procurement team refuses to approve AGPL-licensed software due to copyleft concerns, regardless of technical necessity. A commercial license solves this: you get the exact same Nika codebase, but under terms your legal team can sign.

### 4. OEM / white-label distribution

You want to rebrand Nika, distribute it under your own product name, and keep your modifications private. A commercial license with trademark permissions enables this.

### 5. Need for written warranties, SLAs, or indemnification

AGPL explicitly disclaims warranties. If your enterprise contract requires written warranties, liability caps, indemnification against IP claims, or a response SLA, a commercial license with a support agreement provides all of these.

## What a Commercial License Includes

A Nika commercial license grants:

- **Use rights** to Nika without AGPL copyleft obligations
- **Modification rights** to build proprietary derivative works
- **Distribution rights** for embedding Nika in closed-source products
- **Patent protection** identical to the AGPL grant (same underlying CLA)
- **Optional:** trademark license for "Nika" and the butterfly logo in your product
- **Optional:** priority support, SLA response times, security advisory previews
- **Optional:** custom feature development and private feature flags

Pricing depends on company size, deployment scope, and support level. Startups and small teams get significant discounts. Research institutions get educational rates. Contact us for a quote.

## How Dual Licensing Works Legally

Nika is dual-licensed because contributors sign the [Contributor License Agreement](CLA.md), which grants SuperNovae Studio the right to sublicense their contributions under any license — including commercial terms. This is the same mechanism used by Grafana (AGPL + Commercial), GitLab (MIT + EE), MongoDB (pre-SSPL AGPL + Commercial), and MySQL (GPL + Commercial).

**Key points:**

- Contributors retain full copyright ownership of their contributions.
- Contributors grant SuperNovae a sublicense right (CLA Section 2 and 3).
- SuperNovae can offer the same code under AGPL (public) or commercial terms (paying customers).
- The AGPL version is never removed or degraded — it is always the primary, fully-featured open-source distribution.

This is how dual licensing funds open source sustainability without locking features behind a paywall in the public version.

## AGPL Section 13 — Network Interaction Explained

One specific AGPL requirement that sometimes confuses teams is **Section 13** (the "network clause"):

> if you modify the Program, your modified version must prominently offer all users interacting with it remotely through a computer network […] an opportunity to receive the Corresponding Source of your version […] from a network server at no charge

In practice, if you:

- Run `nika serve` or `nika daemon` unmodified → no obligation (you are running the public source, no modifications to distribute)
- Run a modified version of Nika exposed over a network → you must provide the modified source to users of that network service

For most internal uses, Section 13 is trivial to comply with: do not modify Nika, or publish your modifications on GitHub. If that is not workable for your business, a commercial license removes the obligation.

## Frequently Asked Questions

### Does AGPL affect my workflow YAML files?

No. Workflow files (`.nika.yaml`) are data, not derivative works of Nika itself. You keep full ownership of your workflows under any license you choose. AGPL applies to modifications of Nika's source code (Rust, TypeScript, Lua, configuration), not to what you write with it.

### Can I use Nika to build a commercial product without a commercial license?

Yes, if your product does not modify or link against Nika source code. Examples:

- **Running Nika as a CLI tool in your automation pipeline** — no obligation
- **Calling Nika workflows from your app via HTTP (`nika serve`)** — no obligation (you are a consumer of a network service)
- **Embedding Nika source code inside your closed-source binary** — yes, commercial license needed

### I want to fork Nika and release a competing open-source project

That is allowed under AGPL. You can fork Nika, modify it, and distribute your fork, as long as your fork is also AGPL-licensed and you comply with the CLA.md and LICENSE files in this repository. You may not call your fork "Nika" without a trademark license.

### My company wants written warranties and indemnification

A commercial license plus a support agreement provides both. Contact us.

### Do I need a commercial license to contribute to Nika?

No. Contributors use the same CLA regardless of what license they personally use Nika under. Contributing improvements to the AGPL codebase is strongly encouraged and does not require any payment.

### Does the commercial license affect the open-source version?

No. The AGPL version remains fully featured, actively maintained, and primary. Commercial licenses fund development. There is no "enterprise features" gating in the AGPL version.

### What happens to the commercial license if SuperNovae is acquired?

See [CLA.md](CLA.md) Section 11. License commitments transfer to the successor entity. In the event of dissolution without successor, the AGPL grant survives and continues to benefit the public.

## Contact

- **Commercial licensing:** commercial@supernovae.studio
- **General questions:** contact@supernovae.studio
- **Legal / compliance:** legal@supernovae.studio
- **Security vulnerabilities:** security@supernovae.studio

**SUPERNOVAE** — French Société par Actions Simplifiée (SAS), SIREN 948 452 891, Paris, France.
