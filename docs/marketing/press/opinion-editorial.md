# The AI Industry Needs More Butterflies --- Why AGPL Is the Right License for AI Tools

> *Opinion editorial for technology and policy publications*

---

## The Enclosure Is Already Happening

There is a pattern in technology that repeats with the regularity of a natural law. A community builds something valuable in the open. A corporation takes that thing, wraps it in a proprietary service, captures the value, and gives nothing back. The community that built the foundation is left maintaining the infrastructure while someone else monetizes the building.

This pattern has played out with Linux (the cloud runs on it; cloud providers contribute selectively). It played out with Elasticsearch (Amazon forked it; Elastic changed its license). It played out with MongoDB (cloud database-as-a-service offerings prompted the creation of the SSPL). It played out with Redis (cloud caching services prompted a licensing change).

It is playing out right now, in real time, with AI.

The open source AI community --- researchers, independent developers, startups, and nonprofit labs --- has produced an extraordinary body of work. Open weight models from Meta (Llama), Mistral, DeepSeek, and Hugging Face. Training frameworks, evaluation tools, fine-tuning libraries. Dataset curation pipelines. The entire infrastructure that makes modern AI possible was built, to a significant degree, in the open.

And yet the value capture is asymmetric. Cloud providers offer "AI services" built on open foundations, behind API paywalls, with usage-based pricing. SaaS platforms wrap open source tools in proprietary UIs and charge per seat. Workflow orchestration tools that depend on community-maintained libraries ship under permissive licenses that explicitly allow this capture.

The enclosure is not hypothetical. It is the business model.

---

## The License Is the Architecture

When I chose the license for Nika --- a semantic YAML workflow engine for AI tasks that I have been building as a solo project, now spanning approximately 482,000 lines of Rust --- I chose AGPL-3.0-or-later. Not MIT. Not Apache 2.0. AGPL.

This was not a casual decision. In the startup playbook, AGPL is considered anti-growth. Corporations have policies against it. Package managers flag it. Developers on Hacker News debate whether it is even truly "open source" (it is --- the OSI approved it). The conventional wisdom is that permissive licenses maximize adoption, and adoption is everything.

I reject this framing.

Adoption without protection is not community building. It is resource extraction. When an AI workflow engine is licensed MIT, any cloud provider can take it, deploy it as a service, and capture all the value. The community that reported bugs, contributed patches, wrote documentation, and built integrations gets nothing. The corporation gets a product.

AGPL changes the equation. Its network copyleft provision requires that anyone who modifies the software and provides it as a service over a network must share their modifications under the same license. You can use AGPL software. You can modify it. You can deploy it. But you cannot enclose it. You cannot take what was given freely and lock it behind a proprietary wall.

This is not anti-business. Plenty of businesses thrive on AGPL software. It is anti-extraction. It is the difference between a commons and a quarry.

---

## The One Piece Parallel

Nika is named after the Sun God Nika from Eiichiro Oda's One Piece manga. This is not mere whimsy. The parallel is structural.

In One Piece, the World Government hoards knowledge. It controls the flow of information. It maintains power through enclosure --- the Void Century, the Poneglyphs, the truth about the world. The Marines enforce this system. The Celestial Dragons profit from it. The ordinary people of the world are denied access to knowledge that should belong to everyone.

The pirates fight against this system. Not with a centralized plan, but with a chaotic, distributed, joyful refusal to accept that knowledge and freedom can be enclosed. Whitebeard's last words --- "The One Piece is real!" --- are a declaration that the truth exists and cannot be suppressed. The Sun God Nika, whose power is "limited only by imagination," embodies the principle that liberation is joyful and that the most ridiculous, absurd form of freedom is also the most powerful.

The AI industry in 2026 has its own World Government. A small number of companies control the frontier models, the training compute, the distribution channels, and --- crucially --- the tools that connect users to AI capabilities. The orchestration layer. The workflow engines. The platforms.

Open source AI is the pirate fleet. Messy, underfunded, chaotic, and unstoppable. Mistral dropped frontier model weights via torrent on Twitter. DeepSeek trained a model that matched GPT-4 for $5.6 million and open-sourced it. Hugging Face hosts a million models for free. The community builds, shares, improves, and gives back.

But the pirate fleet has a vulnerability: its ships are mostly licensed MIT or Apache 2.0. Any Marine (cloud provider) can board these ships, take the cannons (code), and turn them against the fleet. The license is the hull. A permissive license is a hull with holes.

AGPL is a hull that holds.

---

## Why Not MIT?

The counterargument to AGPL is straightforward and not unreasonable: permissive licenses maximize adoption. More adoption means more contributors. More contributors means better software. Better software serves more users. It is a virtuous cycle that has produced extraordinary results --- Linux, Python, TensorFlow, PyTorch, Kubernetes.

I do not dispute the historical success of permissive licensing. I dispute its applicability to AI tooling in 2026.

The dynamics have changed. In the 2000s and 2010s, permissive licenses worked because the value chain was distributed. A Linux server ran many applications. No single company captured all the value. The commons was sustained because the commons was useful to everyone, including the corporations contributing to it.

In 2026, the value chain for AI tools is concentrating. Cloud providers do not just use open source --- they provide it as a service. The product is not "software you run" but "software we run for you." The value capture happens at the service layer, not the software layer. And service-layer capture is precisely what permissive licenses allow.

AGPL addresses this by extending copyleft to the service layer. If you provide AGPL software as a network service, your modifications must be shared. This does not prevent commercial use. It prevents extraction without contribution.

The practical objection --- that enterprises will not adopt AGPL software --- is worth examining. Many enterprises have blanket AGPL prohibitions. But these policies are legacy artifacts of an era when copyleft was seen as a viral legal risk. The AGPL's actual requirements are clear and well-understood: if you modify the code and serve it over a network, share your modifications. If you use it internally without serving it externally, no obligations beyond what any open source license requires.

For a tool like Nika --- a CLI binary that users run on their own machines --- the AGPL's network provision is rarely triggered. Users download the binary, run workflows locally, and never serve the software to anyone. The license protects against the specific threat of cloud enclosure without meaningfully restricting normal use.

---

## The Cloud Exploitation Problem

Let me be specific about the threat.

Imagine Nika succeeds. Imagine tens of thousands of users adopt it. They build workflow libraries, contribute showcase templates, file bug reports, write tutorials, create IDE integrations. A community forms. The software improves through collective effort.

Under MIT licensing, Amazon could take Nika, rebrand it as "AWS AI Workflow Service," add proprietary integrations with S3 and Lambda, charge per workflow execution, and give nothing back. Google could do the same with GCP. Microsoft with Azure. They would not need to share a single line of code.

The community that built the foundation would be outcompeted by corporations deploying its own software with superior distribution, support, and integration advantages. This is not speculation. This is exactly what happened with Elasticsearch, MongoDB, and Redis. It is exactly what prompted those projects to change their licenses.

Under AGPL, the same scenario plays out differently. Amazon can still offer Nika as a service. But they must share any modifications they make. The community gets the improvements. The commons is preserved. The value flows both ways.

This is not hostility toward corporations. This is alignment of incentives. AGPL says: you can profit from this software, but you must contribute to the ecosystem that makes it possible. This is fair. This is sustainable. This is how a commons should work.

---

## The "Open Source" Label War

The term "open source" has become contested terrain in the AI industry. Companies release model weights and call them "open source" while restricting commercial use above certain revenue thresholds (Meta's Llama license). Companies release model architectures and call them "open source" while keeping training data proprietary. Companies release tools and call them "open source" while maintaining proprietary cloud services as the primary distribution channel.

The Open Source Initiative (OSI) has clear definitions. The Free Software Foundation has clear definitions. By both standards, AGPL-3.0 is unambiguously open source and unambiguously free software. It grants all four freedoms: to use, study, share, and modify. The only restriction is reciprocity: if you modify and serve, you share.

This is a feature, not a limitation. Reciprocity is what sustains commons. Grazing rights without an obligation to maintain the pasture leads to the tragedy of the commons. AGPL establishes the maintenance obligation.

---

## A Call for More AGPL AI Tools

I am writing this not just to explain my own choice but to advocate for a broader shift. The AI tooling ecosystem needs more AGPL projects. Here is why:

**1. AI tools are infrastructure.** They are the plumbing that connects models to users. Infrastructure should be commons. Commons need protection.

**2. The SaaS model is dominant.** Unlike desktop software, AI tools are increasingly consumed as services. The network copyleft provision is specifically designed for this reality.

**3. The window is closing.** As the AI orchestration market consolidates around a few major platforms, the ability to build independent, community-governed alternatives narrows. AGPL provides the legal foundation for tools that cannot be enclosed.

**4. AGPL projects can be commercially successful.** Grafana Labs (AGPL) is valued at over $6 billion. Nextcloud (AGPL) serves millions of users. GitLab used AGPL for years before changing strategies. The license does not prevent business success --- it prevents a specific kind of exploitation.

**5. The AI community is philosophical.** More than any previous technology wave, AI provokes deep questions about power, access, and governance. The license is a statement of values. AGPL says: we believe in freedom, and we will protect it.

---

## The Butterfly Cannot Be Contained

In One Piece, when the Sun God Nika's power activates, blue butterflies swarm in every direction. They land on the shoulders of enemies, inspiring doubt. They cross battle lines. They cannot be crushed faster than they multiply.

This is the image I hold for open source AI: something small, beautiful, and impossible to contain. A single binary that anyone can download. A YAML file that anyone can write. A license that ensures the code stays free.

The AI industry does not lack intelligence. It does not lack funding. It does not lack ambition. What it lacks is protection for the commons --- the shared infrastructure that everyone depends on and no one owns.

AGPL is that protection. It is not perfect. No license is. But it is the best tool we have for ensuring that the tools we build together remain ours together.

Nika is one butterfly. The AI ecosystem needs a thousand more.

---

## Practical Guidance

For developers and teams considering AGPL for their AI tools:

**When AGPL makes sense:**
- Tools that users run locally (CLI, desktop apps) --- the network provision rarely triggers
- Infrastructure that cloud providers might enclose as a service
- Projects where community contribution is essential to long-term viability
- Tools with a clear commercial-license dual-licensing option for enterprises that need it

**When AGPL may not fit:**
- Libraries designed to be embedded in other software (LGPL or Apache may be more appropriate)
- Projects seeking maximum enterprise adoption with zero legal review friction
- Projects backed by companies with business models that depend on proprietary extensions

**How to mitigate adoption concerns:**
- Make the binary easy to download and run (one curl command)
- Provide clear documentation on what the license does and does not require
- Offer a commercial license for organizations with blanket AGPL prohibitions
- Focus on making the software so good that the license becomes a non-issue

---

## Closing

The choice of license is not a technical decision. It is a political one. It determines who benefits from the work, who can capture the value, and who gets to decide the future direction of the software.

I chose AGPL because I believe AI tools should be free --- not just free to use, but free to remain free. Free from enclosure. Free from extraction. Free in the way that the Sun God Nika is free: joyfully, absurdly, impossibly free.

The butterflies are already flying. The question is whether we protect them.

---

*Thibaut Melen is the founder of SuperNovae Studio and the creator of Nika, a semantic YAML workflow engine for AI tasks licensed under AGPL-3.0-or-later. He can be reached at thibaut@supernovae.studio.*
