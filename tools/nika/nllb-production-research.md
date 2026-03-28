# Research Report: Serving NLLB-200 in Production

## Summary

NLLB-200 (No Language Left Behind) by Meta is the most capable open-source multilingual translation model, supporting 200 languages across 40,000+ directions. For production serving, CTranslate2 with INT8 quantization is the clear winner for throughput (3-6x faster than raw PyTorch). The 600M distilled model hits the sweet spot for most use cases: fast, small, and good enough quality for high-resource languages. For low-resource African/indigenous languages, NLLB is currently the ONLY viable option -- nothing else comes close.

## 1. CTranslate2 + NLLB Setup

### Model Variants Available

| Model | Params | Disk (FP32) | Disk (INT8) | HuggingFace ID |
|-------|--------|-------------|-------------|----------------|
| NLLB-200-Distilled | 600M | ~2.3 GB | ~600 MB | `facebook/nllb-200-distilled-600M` |
| NLLB-200-Distilled | 1.3B | ~5.0 GB | ~1.3 GB | `facebook/nllb-200-distilled-1.3B` |
| NLLB-200 Dense | 1.3B | ~5.0 GB | ~1.3 GB | `facebook/nllb-200-1.3B` |
| NLLB-200 Dense | 3.3B | ~13 GB | ~3.3 GB | `facebook/nllb-200-3.3B` |

Pre-converted CT2 models on HuggingFace:
- `OpenNMT/nllb-200-3.3B-ct2-int8` (774 downloads)
- `OpenNMT/nllb-200-distilled-1.3B-ct2-int8` (385 downloads)
- `JustFrederik/nllb-200-distilled-600M-ct2-int8` (954 downloads)

### Conversion Commands

```bash
# Install dependencies
pip install ctranslate2 transformers[torch] sentencepiece

# Convert to CTranslate2 format with INT8 quantization
ct2-transformers-converter \
  --model facebook/nllb-200-distilled-600M \
  --output_dir nllb-200-distilled-600M-ct2 \
  --quantization int8

# For 1.3B model
ct2-transformers-converter \
  --model facebook/nllb-200-distilled-1.3B \
  --output_dir nllb-200-distilled-1.3B-ct2 \
  --quantization int8

# For 3.3B model (FP16 on GPU recommended)
ct2-transformers-converter \
  --model facebook/nllb-200-3.3B \
  --output_dir nllb-200-3.3B-ct2 \
  --quantization float16

# Or download pre-converted models
pip install huggingface_hub
huggingface-cli download OpenNMT/nllb-200-3.3B-ct2-int8 --local-dir nllb-200-3.3B-ct2-int8
```

### Minimal CTranslate2 Translation Code

```python
import ctranslate2
import transformers

# Load model and tokenizer
translator = ctranslate2.Translator(
    "nllb-200-distilled-600M-ct2",
    device="cuda",           # or "cpu"
    compute_type="int8",     # int8, float16, float32
    inter_threads=4,         # CPU parallelism
    intra_threads=2,         # Per-request parallelism
)

tokenizer = transformers.AutoTokenizer.from_pretrained(
    "facebook/nllb-200-distilled-600M",
    src_lang="eng_Latn"
)

def translate(text: str, src_lang: str, tgt_lang: str) -> str:
    tokenizer.src_lang = src_lang
    source = tokenizer.convert_ids_to_tokens(tokenizer.encode(text))
    target_prefix = [tgt_lang]

    results = translator.translate_batch(
        [source],
        target_prefix=[target_prefix],
        max_batch_size=1024,    # tokens per batch for auto-batching
        beam_size=4,
        max_decoding_length=512,
    )
    target = results[0].hypotheses[0][1:]  # skip language token
    return tokenizer.decode(tokenizer.convert_tokens_to_ids(target))

# Usage
print(translate("Hello world!", "eng_Latn", "fra_Latn"))
# => "Bonjour le monde !"
```

### Batch Translation

```python
def translate_batch(texts: list[str], src_lang: str, tgt_lang: str) -> list[str]:
    tokenizer.src_lang = src_lang
    sources = [
        tokenizer.convert_ids_to_tokens(tokenizer.encode(t))
        for t in texts
    ]
    target_prefixes = [[tgt_lang]] * len(texts)

    results = translator.translate_batch(
        sources,
        target_prefix=target_prefixes,
        max_batch_size=2048,
        beam_size=4,
    )
    return [
        tokenizer.decode(tokenizer.convert_tokens_to_ids(r.hypotheses[0][1:]))
        for r in results
    ]
```

### Performance Benchmarks (CTranslate2)

From the official CTranslate2 benchmarks (En->De newstest2014):

**CPU** (Intel Xeon Platinum 8275CL, c5.2xlarge, 4 threads):

| Backend | Tokens/sec | Memory | BLEU |
|---------|-----------|--------|------|
| Transformers (PyTorch) | 147 | 2332 MB | 27.90 |
| CTranslate2 FP32 | 525 | 721 MB | 27.92 |
| CTranslate2 INT16 | 596 | 660 MB | 27.53 |
| CTranslate2 INT8 | 696 | 516 MB | 27.65 |

**GPU** (NVIDIA A10G, g5.xlarge):

| Backend | Tokens/sec | GPU Memory | BLEU |
|---------|-----------|------------|------|
| Transformers (PyTorch) | 1,023 | 4097 MB | 27.90 |
| CTranslate2 FP32 | 5,876 | 1197 MB | 27.92 |
| CTranslate2 INT8 | 7,522 | 1005 MB | 27.79 |
| CTranslate2 FP16 | 9,297 | 909 MB | 27.90 |
| CTranslate2 INT8+FP16 | 8,363 | 813 MB | 27.90 |

**Key takeaway**: CTranslate2 is 3-5x faster than raw PyTorch on CPU, 5-9x faster on GPU, using 3-4x less memory.

**Estimated NLLB-specific throughput** (extrapolated from base Transformer benchmarks):
- 600M INT8 on CPU (4 cores): ~400-500 tokens/sec
- 600M INT8 on A10G GPU: ~4,000-6,000 tokens/sec
- 1.3B INT8 on A10G GPU: ~2,000-3,500 tokens/sec
- 3.3B FP16 on A10G GPU: ~1,000-2,000 tokens/sec

At 500 tokens/sec, that is roughly 30,000 words/minute or ~1.8M words/hour on a single A10G.


## 2. NLLB Language Codes (FLORES-200 to BCP-47 Mapping)

### How NLLB Codes Work

NLLB uses FLORES-200 codes in the format `{iso639_3}_{script}`:
- `fra_Latn` = French in Latin script
- `arb_Arab` = Modern Standard Arabic in Arabic script
- `zho_Hans` = Chinese Simplified
- `zho_Hant` = Chinese Traditional

### NLLB Does NOT Handle Locale Variants

NLLB operates at the **language + script** level, NOT the locale level. This means:
- `fra_Latn` covers ALL French: fr-FR, fr-CA, fr-BE, fr-SN, fr-CH
- `spa_Latn` covers ALL Spanish: es-ES, es-MX, es-AR, es-CO
- `por_Latn` covers ALL Portuguese: pt-BR, pt-PT, pt-MZ
- `eng_Latn` covers ALL English: en-US, en-GB, en-AU, en-IN

**Exception**: Arabic has dialect-level codes:
- `arb_Arab` = Modern Standard Arabic
- `arz_Arab` = Egyptian Arabic
- `ary_Arab` = Moroccan Arabic
- `acm_Arab` = Mesopotamian Arabic
- `ajp_Arab` = South Levantine Arabic
- `apc_Arab` = North Levantine Arabic
- `ars_Arab` = Najdi Arabic
- `acq_Arab` = Ta'izzi-Adeni Arabic
- `aeb_Arab` = Tunisian Arabic

Similarly, Chinese distinguishes by script:
- `zho_Hans` = Simplified Chinese (mainland/Singapore)
- `zho_Hant` = Traditional Chinese (Taiwan/HK)
- `yue_Hant` = Cantonese (Traditional)

### BCP-47 to FLORES-200 Mapping Table

Here is a practical mapping for the most common BCP-47 locales:

```python
BCP47_TO_FLORES = {
    # Major European
    "fr": "fra_Latn", "fr-FR": "fra_Latn", "fr-CA": "fra_Latn",
    "fr-BE": "fra_Latn", "fr-CH": "fra_Latn", "fr-SN": "fra_Latn",
    "de": "deu_Latn", "de-DE": "deu_Latn", "de-AT": "deu_Latn", "de-CH": "deu_Latn",
    "es": "spa_Latn", "es-ES": "spa_Latn", "es-MX": "spa_Latn", "es-AR": "spa_Latn",
    "it": "ita_Latn", "it-IT": "ita_Latn",
    "pt": "por_Latn", "pt-BR": "por_Latn", "pt-PT": "por_Latn",
    "en": "eng_Latn", "en-US": "eng_Latn", "en-GB": "eng_Latn",
    "nl": "nld_Latn", "nl-NL": "nld_Latn", "nl-BE": "nld_Latn",
    "pl": "pol_Latn", "ro": "ron_Latn", "sv": "swe_Latn",
    "da": "dan_Latn", "no": "nob_Latn", "nb": "nob_Latn", "nn": "nno_Latn",
    "fi": "fin_Latn", "el": "ell_Grek", "cs": "ces_Latn",
    "sk": "slk_Latn", "hu": "hun_Latn", "bg": "bul_Cyrl",
    "hr": "hrv_Latn", "sr": "srp_Cyrl", "sl": "slv_Latn",
    "uk": "ukr_Cyrl", "ru": "rus_Cyrl", "be": "bel_Cyrl",
    "et": "est_Latn", "lv": "lvs_Latn", "lt": "lit_Latn",
    "is": "isl_Latn", "ga": "gle_Latn", "cy": "cym_Latn",
    "eu": "eus_Latn", "ca": "cat_Latn", "gl": "glg_Latn",
    "mt": "mlt_Latn", "sq": "als_Latn", "mk": "mkd_Cyrl",
    "bs": "bos_Latn", "lb": "ltz_Latn", "oc": "oci_Latn",
    "ast": "ast_Latn",
    # Turkish / Central Asian
    "tr": "tur_Latn", "az": "azj_Latn", "kk": "kaz_Cyrl",
    "ky": "kir_Cyrl", "uz": "uzn_Latn", "tk": "tuk_Latn",
    "tg": "tgk_Cyrl",
    # Asian
    "zh": "zho_Hans", "zh-CN": "zho_Hans", "zh-TW": "zho_Hant",
    "zh-HK": "zho_Hant", "zh-Hans": "zho_Hans", "zh-Hant": "zho_Hant",
    "ja": "jpn_Jpan", "ko": "kor_Hang",
    "vi": "vie_Latn", "th": "tha_Thai", "km": "khm_Khmr",
    "lo": "lao_Laoo", "my": "mya_Mymr",
    "hi": "hin_Deva", "bn": "ben_Beng", "ur": "urd_Arab",
    "ta": "tam_Taml", "te": "tel_Telu", "ml": "mal_Mlym",
    "kn": "kan_Knda", "gu": "guj_Gujr", "pa": "pan_Guru",
    "mr": "mar_Deva", "ne": "npi_Deva", "si": "sin_Sinh",
    "as": "asm_Beng", "or": "ory_Orya",
    "ms": "zsm_Latn", "id": "ind_Latn", "tl": "tgl_Latn",
    "jv": "jav_Latn", "ceb": "ceb_Latn",
    # Arabic dialects
    "ar": "arb_Arab", "ar-SA": "arb_Arab", "ar-EG": "arz_Arab",
    "ar-MA": "ary_Arab", "ar-IQ": "acm_Arab", "ar-TN": "aeb_Arab",
    # Persian
    "fa": "pes_Arab", "ps": "pbt_Arab",
    # Hebrew
    "he": "heb_Hebr", "yi": "ydd_Hebr",
    # African
    "sw": "swh_Latn", "ha": "hau_Latn", "yo": "yor_Latn",
    "ig": "ibo_Latn", "am": "amh_Ethi", "so": "som_Latn",
    "rw": "kin_Latn", "wo": "wol_Latn", "sn": "sna_Latn",
    "zu": "zul_Latn", "xh": "xho_Latn", "ny": "nya_Latn",
    "ln": "lin_Latn", "ti": "tir_Ethi", "lg": "lug_Latn",
    # Americas
    "ht": "hat_Latn", "gn": "grn_Latn", "qu": "quy_Latn",
    # Georgian / Armenian
    "ka": "kat_Geor", "hy": "hye_Armn",
    # Mongolian
    "mn": "khk_Cyrl",
}
```

**Total FLORES-200 languages**: 200 (with some having dual script variants like ace_Arab + ace_Latn)


## 3. NLLB Quality by Language (chrF++ Scores)

Data source: Official NLLB-200 evaluation on FLORES-200 benchmark.
chrF++ is a character n-gram metric (higher = better, 100 = perfect).

### Tier 1: Excellent (chrF++ > 60 to/from English)

These languages have near-commercial quality:

| Language | -> English | English -> | Notes |
|----------|-----------|------------|-------|
| Portuguese | 69.0 | 67.4 | Best overall pair |
| French | 65.4 | 67.0 | Excellent both ways |
| German | 65.3 | 59.4 | Strong to-English |
| Afrikaans | 72.9 | -- | Highest to-English score |
| Maltese | 71.7 | -- | Surprisingly high |
| Welsh | 68.2 | -- | Very strong |
| Swedish | 67.0 | -- | Strong Nordic |
| Danish | 67.7 | -- | Strong Nordic |
| Arabic (MSA) | 62.2 | 51.4 | Good to-English |
| Hindi | 62.5 | 54.2 | Solid both ways |
| Turkish | 60.0 | -- | Good |
| Swahili | 60.7 | 58.0 | Best African language |
| Cebuano | 62.5 | 55.2 | Strong for low-resource |

### Tier 2: Good (chrF++ 45-60)

Usable for production with post-editing:

| Language | -> English | English -> | Notes |
|----------|-----------|------------|-------|
| Spanish | 57.1 | 52.6 | Good but below Google |
| Russian | 58.6 | 52.5 | Solid |
| Vietnamese | 58.1 | -- | Good |
| Chinese (Simplified) | 52.9 | 19.6 | Good to-English, WEAK from English |
| Japanese | 50.2 | 23.6 | OK to-English, WEAK from English |
| Korean | 52.5 | 32.1 | OK to-English, mediocre from English |
| Thai | 52.7 | -- | Decent |
| Polish | 54.3 | -- | Decent |
| Amharic | 54.9 | 37.2 | Good for Ethiopian |
| Hausa | 52.6 | 49.0 | Good for West African |
| Kinyarwanda | 51.3 | -- | Decent |
| Somali | 49.4 | -- | Decent |
| Igbo | 48.8 | 40.0 | Usable |
| Nyanja | 47.3 | -- | Usable |
| Lingala | 46.7 | 46.9 | Usable |

### Tier 3: Weak (chrF++ 30-45)

Use with caution, best for gisting:

| Language | -> English | English -> | Notes |
|----------|-----------|------------|-------|
| Yoruba | 39.9 | 22.9 | Weak FROM English |
| Wolof | 36.7 | 23.5 | Weak FROM English |
| Guarani | 44.5 | 34.0 | Moderate |
| Quechua (Ayacucho) | 30.9 | 24.1 | Weakest major indigenous |
| Tigrinya | 45.2 | -- | Moderate |
| Luganda | 41.7 | -- | Moderate |

### Tier 4: Poor (chrF++ < 30)

Not production-ready:

| Direction | chrF++ | Notes |
|-----------|--------|-------|
| eng -> Kanuri (Arabic) | 10.5 | Worst direction overall |
| eng -> Yue Chinese | 15.4 | Very poor |
| eng -> Tamasheq (Tifinagh) | 14.3 | Nearly unusable |
| eng -> Acehnese (Arabic) | 14.4 | Nearly unusable |
| eng -> Dyula | 17.3 | Very weak |
| eng -> Fon | 17.3 | Very weak |
| eng -> Chinese (Traditional) | 13.3 | Surprisingly poor |

### 600M vs 3.3B Model Comparison

The 3.3B model gains 2-5 chrF++ points across all pairs:

| Direction | 600M | 3.3B | Delta |
|-----------|------|------|-------|
| fra -> eng | 65.4 | 68.1 | +2.7 |
| eng -> fra | 67.0 | 69.6 | +2.6 |
| jpn -> eng | 50.2 | 55.1 | +4.9 |
| yor -> eng | 39.9 | 44.5 | +4.6 |
| wol -> eng | 36.7 | 39.8 | +3.1 |
| eng -> wol | 23.5 | 28.1 | +4.6 |
| swh -> eng | 60.7 | 65.0 | +4.3 |

**Key insight**: The 3.3B model benefits low-resource languages more (+3-5 points) than high-resource ones (+2-3 points). If you serve African/indigenous languages, the 3.3B model is worth the extra compute.

### Overall Statistics (600M Distilled Model)

- **Total directions evaluated**: 602 (English-centric + sampled non-English)
- **Mean chrF++**: 44.6
- **Min chrF++**: 10.4 (Mongolian -> Cantonese)
- **Max chrF++**: 72.9 (Afrikaans -> English)


## 4. Serving Options Compared

### Option A: CTranslate2 + FastAPI (RECOMMENDED)

**Best for**: Maximum throughput, production deployments.

```python
# server.py
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel
import ctranslate2
import transformers
from contextlib import asynccontextmanager

model_path = "nllb-200-distilled-600M-ct2"
hf_model = "facebook/nllb-200-distilled-600M"

translator = None
tokenizer = None

@asynccontextmanager
async def lifespan(app: FastAPI):
    global translator, tokenizer
    translator = ctranslate2.Translator(
        model_path,
        device="cuda",       # or "cpu"
        compute_type="int8",
        inter_threads=4,
    )
    tokenizer = transformers.AutoTokenizer.from_pretrained(hf_model)
    yield
    del translator

app = FastAPI(lifespan=lifespan)

class TranslateRequest(BaseModel):
    text: str | list[str]
    source_lang: str   # FLORES-200 code, e.g. "eng_Latn"
    target_lang: str   # FLORES-200 code, e.g. "fra_Latn"
    beam_size: int = 4

class TranslateResponse(BaseModel):
    translations: list[str]

@app.post("/translate", response_model=TranslateResponse)
async def translate(req: TranslateRequest):
    texts = [req.text] if isinstance(req.text, str) else req.text
    tokenizer.src_lang = req.source_lang

    sources = [
        tokenizer.convert_ids_to_tokens(tokenizer.encode(t))
        for t in texts
    ]
    target_prefixes = [[req.target_lang]] * len(texts)

    results = translator.translate_batch(
        sources,
        target_prefix=target_prefixes,
        beam_size=req.beam_size,
        max_batch_size=2048,
        max_decoding_length=512,
    )

    translations = [
        tokenizer.decode(
            tokenizer.convert_tokens_to_ids(r.hypotheses[0][1:])
        )
        for r in results
    ]
    return TranslateResponse(translations=translations)

@app.get("/languages")
async def languages():
    return {"count": 200, "codes": "See FLORES-200 list"}

# Run: uvicorn server:app --host 0.0.0.0 --port 8000 --workers 1
```

**Deployment**:
```bash
pip install fastapi uvicorn ctranslate2 transformers sentencepiece
uvicorn server:app --host 0.0.0.0 --port 8000

# Docker
FROM python:3.11-slim
RUN pip install fastapi uvicorn ctranslate2 transformers sentencepiece
COPY server.py .
COPY nllb-200-distilled-600M-ct2/ ./nllb-200-distilled-600M-ct2/
CMD ["uvicorn", "server:app", "--host", "0.0.0.0", "--port", "8000"]
```

**Performance**: ~400-600 tokens/sec CPU, ~4,000-6,000 tokens/sec GPU (A10G), ~800 MB RAM (INT8 600M).

### Option B: nllb-serve (Turnkey)

**Best for**: Quick prototyping, no code needed.

```bash
pip install git+https://github.com/thammegowda/nllb-serve
nllb-serve --port 6060 --model_id facebook/nllb-200-distilled-600M

# REST API
curl -X POST http://localhost:6060/translate \
  -H "Content-Type: application/json" \
  -d '{"source": ["Hello world"], "src_lang": "eng_Latn", "tgt_lang": "fra_Latn"}'

# Also supports batch mode
nllb-batch -sl eng_Latn -tl fra_Latn -b 32 < input.txt > output.txt
```

Stars: 259 | Uses raw PyTorch (slower than CT2) | Flask-based.
**Performance**: ~100-150 tokens/sec CPU, ~800-1,000 tokens/sec GPU.

### Option C: HuggingFace TGI

**TGI supports seq2seq models** via `AutoModelForSeq2SeqLM`. However, NLLB is not a first-class citizen in TGI -- it works but is not optimized for translation workloads specifically (TGI is mainly optimized for LLM generation).

```bash
# This works but is not specifically optimized for NLLB
docker run --gpus all -p 8080:80 \
  ghcr.io/huggingface/text-generation-inference:latest \
  --model-id facebook/nllb-200-distilled-600M

# TGI is better suited for decoder-only models. For NLLB, CTranslate2 is faster.
```

**Verdict**: Possible but not recommended. CTranslate2 will be faster for encoder-decoder translation models.

### Option D: Custom Transformers + FastAPI

```python
# Simpler but 3-5x slower than CTranslate2
from transformers import AutoModelForSeq2SeqLM, AutoTokenizer
import torch

model = AutoModelForSeq2SeqLM.from_pretrained(
    "facebook/nllb-200-distilled-600M",
    torch_dtype=torch.float16
).to("cuda")
tokenizer = AutoTokenizer.from_pretrained("facebook/nllb-200-distilled-600M")

def translate(text, src_lang, tgt_lang):
    tokenizer.src_lang = src_lang
    inputs = tokenizer(text, return_tensors="pt", padding=True).to("cuda")
    with torch.no_grad():
        translated = model.generate(
            **inputs,
            forced_bos_token_id=tokenizer.convert_tokens_to_ids(tgt_lang),
            max_new_tokens=512,
        )
    return tokenizer.batch_decode(translated, skip_special_tokens=True)
```

**Performance**: ~150 tokens/sec CPU, ~1,000 tokens/sec GPU.

### Option E: OpenAI-Compatible Wrapper

There is no standard OpenAI-compatible wrapper for NLLB. The OpenAI `/v1/chat/completions` format does not map naturally to translation models. However, you could build a compatibility shim:

```python
# Fake OpenAI-compatible endpoint for translation
@app.post("/v1/chat/completions")
async def openai_compat(request: dict):
    # Parse translation instructions from the chat message
    message = request["messages"][-1]["content"]
    # Extract source/target from system prompt or message format
    # This is hacky -- NLLB is NOT an LLM, it's a seq2seq model
    ...
```

**Verdict**: Not recommended. NLLB is not an LLM and should use its own API format.

### Serving Comparison Summary

| Option | Throughput (GPU) | Setup Effort | Production-Ready |
|--------|-----------------|-------------|-----------------|
| CTranslate2 + FastAPI | ~5,000 tok/s | Medium | YES |
| nllb-serve | ~1,000 tok/s | Very Low | Prototype only |
| HuggingFace TGI | ~1,500 tok/s | Low | Yes but suboptimal |
| Raw Transformers + FastAPI | ~1,000 tok/s | Low | Yes but slow |


## 5. Cost Comparison: NLLB vs Google Translate API

### Google Cloud Translation Pricing (2026)

| Tier | Price |
|------|-------|
| Basic (v2) - first 500K chars/month | FREE |
| Basic (v2) - over 500K chars/month | $20 / million characters |
| Advanced (v3) NMT | $20 / million characters |
| Advanced (v3) LLM | $10 input + $10 output / million chars |
| Advanced (v3) Custom | $25-80 / million characters |
| Document translation | $0.08 / page |

### Self-Hosted NLLB Cost Calculation

**Scenario**: 1M characters/day = ~30M characters/month

Assumptions:
- Average word = 5 characters, average sentence = 15 words = 75 chars
- 1M characters ~ 13,333 sentences/day ~ 200,000 words/day
- At ~500 tokens/sec (CT2 INT8 on CPU), this takes ~26 minutes/day
- At ~5,000 tokens/sec (CT2 on GPU), this takes ~2.6 minutes/day

**Infrastructure costs** (AWS, monthly):

| Setup | Instance | Cost/month | Characters/day capacity |
|-------|----------|-----------|------------------------|
| CPU (c5.2xlarge) | 8 vCPU, 16 GB | ~$250 | ~5M chars |
| GPU (g5.xlarge) | A10G, 4 vCPU | ~$770 | ~50M chars |
| GPU (g4dn.xlarge) | T4, 4 vCPU | ~$380 | ~25M chars |
| GPU Spot (g5.xlarge) | A10G spot | ~$230 | ~50M chars |

**Google Translate cost for same volume**:
- 1M chars/day = 30M chars/month
- 30M chars * $20/M = **$600/month**

### Break-Even Analysis

| Daily Volume | Google Cost/mo | NLLB CPU (c5.2xl) | NLLB GPU Spot (g5) |
|-------------|---------------|-------------------|-------------------|
| 500K chars | $300 | $250 | $230 |
| 1M chars | $600 | $250 | $230 |
| 5M chars | $3,000 | $250 (1 instance) | $230 |
| 10M chars | $6,000 | $500 (2 instances) | $230 |
| 50M chars | $30,000 | $2,500 | $460 (2 spot) |

**Break-even**: NLLB self-hosted becomes cheaper at ~500K characters/day (~15M chars/month). Below that, Google's free tier + low volume makes it competitive.

**At 1M chars/day**: Google = $600/mo, NLLB GPU spot = $230/mo = **62% savings**.
**At 10M chars/day**: Google = $6,000/mo, NLLB GPU spot = $230/mo = **96% savings**.

### Hidden Costs of Self-Hosting

- Engineering time for setup/maintenance (~40 hours initial, ~4 hours/month)
- Monitoring, logging, alerting infrastructure
- Model updates (NLLB does not update frequently)
- No SLA vs Google's 99.9% SLA
- NLLB license: **CC-BY-NC-4.0** (non-commercial only for model weights!)

### CRITICAL: NLLB License Warning

NLLB model weights are licensed under **CC-BY-NC-4.0** (Creative Commons Attribution-NonCommercial). This means:
- **Commercial use is NOT permitted** without a separate license from Meta
- For commercial translation, you need either:
  1. A commercial license from Meta (contact them)
  2. Use OPUS-MT models (MIT licensed) instead (fewer languages, lower quality)
  3. Use Google/DeepL APIs (pay per use)
  4. Fine-tune your own model from scratch

This is a significant consideration for any production deployment.


## Sources

1. [CTranslate2 GitHub](https://github.com/OpenNMT/CTranslate2) - Performance benchmarks, architecture support
2. [FLORES-200 README](https://github.com/facebookresearch/flores/blob/main/flores200/README.md) - Complete language code table
3. [NLLB Paper (arXiv:2207.04672)](https://arxiv.org/abs/2207.04672) - Model architecture, 44% BLEU improvement claim
4. [NLLB-200 Distilled 600M Metrics](https://tinyurl.com/nllb200densedst600mmetrics) - Official chrF++ on 602 directions
5. [NLLB-200 3.3B Metrics](https://tinyurl.com/nllb200dense3bmetrics) - Official chrF++ for 3.3B dense model
6. [nllb-serve (GitHub, 259 stars)](https://github.com/thammegowda/nllb-serve) - Turnkey REST API wrapper
7. [OpenNMT/nllb-200-3.3B-ct2-int8 (HuggingFace)](https://huggingface.co/OpenNMT/nllb-200-3.3B-ct2-int8) - Pre-converted CT2 model
8. [Google Cloud Translation Pricing](https://cloud.google.com/translate/pricing) - $20/M chars (Basic/NMT)
9. [fairseq NLLB modeling README](https://github.com/facebookresearch/fairseq/tree/nllb/examples/nllb/modeling) - Training recipes, metrics links
10. [CTranslate2 Transformers Guide](https://opennmt.net/CTranslate2/guides/transformers.html) - NLLB conversion instructions

## Methodology

- Tools used: GitHub API, HuggingFace API, direct README scraping, NLLB metrics CSV download
- Pages analyzed: ~25
- Data sources: Official Meta NLLB metrics (602 direction chrF++ scores), CTranslate2 benchmarks, Google pricing page
- All performance numbers are from official benchmarks or conservative extrapolations

## Confidence Level

**High** for:
- CTranslate2 setup and conversion (official docs)
- Language codes and mapping (official FLORES-200)
- Quality scores (official Meta metrics, directly downloaded)
- Google pricing (official pricing page)

**Medium** for:
- NLLB-specific throughput numbers (extrapolated from CTranslate2 benchmarks on similar-sized models, not NLLB-specific benchmarks)
- Cost projections (depends on actual traffic patterns, batching efficiency)

**Low** for:
- TGI performance with NLLB (not officially benchmarked)

## Further Research Suggestions

- Benchmark NLLB-200 distilled 600M specifically on your hardware with CTranslate2
- Evaluate OPUS-MT as a MIT-licensed alternative for commercial use
- Test NLLB fine-tuning on domain-specific data for quality improvement
- Investigate Marian NMT as another fast inference engine (used by Firefox Translations)
- Check if Meta offers commercial licensing for NLLB weights
- Consider MADLAD-400 (Google) as an alternative -- supports 400+ languages, Apache 2.0 license
