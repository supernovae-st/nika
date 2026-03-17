# MCP Media Servers Catalog (March 2026)

Comprehensive catalog of MCP servers that handle non-text media, organized by media type.

---

## 1. Image Generation

### mcp-image
- **npm**: `mcp-image` (v0.8.1)
- **GitHub**: https://github.com/shinpr/mcp-image (inferred from maintainer)
- **Media**: AI image generation + editing (text-to-image, image-to-image)
- **Backend**: Google Gemini (Nano Banana 2 / Nano Banana Pro models)
- **Output**: Saves files to disk (PNG, JPEG, WebP). Path returned in response.
- **Key Tools**:
  - `generate_image` -- text prompt to image with auto prompt enhancement
  - Image editing (image-to-image with natural language instructions)
- **Params**: prompt, quality preset (fast/balanced/quality), aspect ratio (1:1 to 21:9), output format, character consistency, multi-image blending
- **Data Return**: File path on disk (configurable via `IMAGE_OUTPUT_DIR`)
- **Notes**: Built-in Subject-Context-Style prompt optimizer via Gemini 2.5 Flash. Up to 4K output. Also ships an Agent Skill (`SKILL.md`) for prompt engineering education.

### comfyui-mcp
- **npm**: `comfyui-mcp` (v0.3.2)
- **GitHub**: https://github.com/artokun/comfyui-mcp
- **Media**: Full ComfyUI pipeline -- image generation, upscaling, inpainting, img2img
- **Backend**: ComfyUI (Stable Diffusion 1.5, SDXL, Flux, SD3, LTXV, etc.)
- **Output**: Images saved to ComfyUI output directory. File paths returned.
- **Key Tools** (31 total):
  - `enqueue_workflow` -- submit workflow JSON for execution
  - `create_workflow` -- generate txt2img/img2img/upscale/inpaint workflows from templates
  - `modify_workflow` -- set inputs, add/remove nodes, connect
  - `validate_workflow` -- dry-run validation
  - `upload_image` -- copy local image into ComfyUI input dir
  - `list_output_images` -- browse generated images
  - `search_models` / `download_model` -- HuggingFace model search + download
  - `clear_vram` -- free GPU memory
  - `visualize_workflow` -- Mermaid flowchart from workflow
  - `search_custom_nodes` / `get_node_pack_details` -- ComfyUI Registry
  - `suggest_settings` -- recommend sampler/scheduler/CFG from generation history
- **Params**: Full ComfyUI workflow API (any node, any model)
- **Data Return**: File paths on disk (ComfyUI output directory)
- **Notes**: Also a Claude Code plugin with 10 slash commands, 3 agents, 4 skills, 3 hooks. Most comprehensive image gen MCP server. Supports any ComfyUI model/workflow.

### @ricardopera/mcp-image-server
- **npm**: `@ricardopera/mcp-image-server` (v2.1.1)
- **GitHub**: https://github.com/ricardopera/mcp-image-server (inferred)
- **Media**: Image + icon generation via GPT Image 1 (OpenAI)
- **Backend**: OpenAI GPT Image 1
- **Output**: Files saved to disk in PNG, SVG (embedded base64), ICO (real conversion)
- **Key Tools**:
  - Image generation from text prompts
  - Icon generation with transparent background support
- **Params**: prompt, format (png/svg/ico), size (1024x1024, 1536x1024, 1024x1536), background (transparent/opaque), fileName, directory
- **Data Return**: File path on disk
- **Notes**: Designed for vibe coding asset generation (favicons, logos, icons). Requires `OPENAI_API_KEY`.

### mcp-fal-ai-image
- **npm**: `mcp-fal-ai-image` (v1.0.0)
- **GitHub**: https://github.com/madhusudan-kulkarni/mcp-fal-ai-image
- **Media**: Text-to-image via any fal.ai model
- **Backend**: fal.ai (supports Kolors, Recraft v3, Flux Schnell, and any fal.ai model ID)
- **Output**: Files saved locally (default `~/Downloads/fal_ai`)
- **Key Tools**:
  - `generate-image` -- text to image with model selection
- **Params**: prompt, model ID (any valid fal.ai model), number of images, image size, inference steps, guidance scale, safety checker
- **Data Return**: Local file path in response
- **Notes**: Gateway to entire fal.ai model catalog. Requires `FAL_KEY`.

### @iflow-mcp/mcp-replicate-flux
- **npm**: `@iflow-mcp/mcp-replicate-flux`
- **GitHub**: https://github.com/andylee20014/mcp-replicate-flux
- **Media**: Image generation via Replicate FLUX Schnell model
- **Backend**: Replicate (black-forest-labs/flux-schnell) + Cloudflare R2 storage
- **Output**: URL (uploaded to Cloudflare R2)
- **Key Tools**:
  - `generate-image` -- prompt + filename
- **Params**: prompt, filename
- **Data Return**: Public URL on Cloudflare R2
- **Notes**: Requires Replicate API token + Cloudflare R2 credentials. Returns accessible image URLs.

### mcp-replicate (Generic)
- **npm**: `mcp-replicate` (v0.1.1)
- **GitHub**: https://github.com/deepfates/mcp-replicate
- **Media**: Any Replicate model (image, video, audio, etc.)
- **Backend**: Replicate API (any model)
- **Output**: Varies by model -- URLs for generated content
- **Key Tools**:
  - `search_models` -- semantic search for models
  - `create_prediction` -- run any model with inputs
  - `get_prediction` -- check status
  - `view_image` -- open generated image in browser
  - `clear_image_cache` / `get_image_cache_stats`
- **Params**: Model-dependent (version ID + input dict)
- **Data Return**: URLs from Replicate CDN; local image cache for viewing
- **Notes**: Universal Replicate gateway. Can run any model -- Flux, SDXL, Whisper, video models, etc.

### replicate-mcp (Official)
- **npm**: `replicate-mcp` (v0.9.0)
- **GitHub**: Official Replicate MCP server
- **Media**: Any Replicate model
- **Backend**: Replicate HTTP API
- **Output**: URLs
- **Notes**: Official server from Replicate team. Broader API coverage than community version.

---

## 2. Image Processing

### @iflow-mcp/inhiblabcore-mcp-image-compression
- **npm**: `@iflow-mcp/inhiblabcore-mcp-image-compression` (v0.1.2)
- **GitHub**: https://github.com/inhiblabcore/mcp-image-compression (inferred)
- **Media**: Image compression (JPEG, PNG, WebP, AVIF)
- **Output**: Compressed image URLs
- **Key Tools**:
  - `image_compression` -- compress images with quality/format control
- **Params**: urls (array of image URLs), quality (0-100), format (jpeg/png/webp/avif)
- **Data Return**: Compressed image URLs
- **Notes**: Offline usage supported. Smart compression auto-selects optimal params. Batch processing supported. Configurable download directory via `IMAGE_COMPRESSION_DOWNLOAD_DIR`.

### mcp-image-extractor
- **npm**: `mcp-image-extractor` (v1.1.0)
- **Media**: Extract and convert images to base64 for LLM analysis
- **Output**: Base64 encoded image data
- **Key Tools**:
  - Image extraction and base64 conversion
- **Data Return**: Base64 string in MCP response
- **Notes**: Utility for feeding images into LLMs that accept base64 input.

### @sethdouglasford/mcp-figma (Design Read/Write)
- **npm**: `@sethdouglasford/mcp-figma` (v1.0.9)
- **GitHub**: https://github.com/sethdouglasford/mcp-figma (inferred)
- **Media**: Figma design file read/write
- **Backend**: Figma API + Figma Plugin (WebSocket bridge)
- **Key Tools**:
  - Read Figma designs (nodes, styles, components)
  - Modify designs programmatically
  - Export design assets
- **Data Return**: Figma node data, exported image URLs
- **Notes**: Works with Cursor, VS Code, Claude Desktop. Requires Figma plugin + WebSocket bridge.

### mcp-figma
- **npm**: `mcp-figma` (v0.1.1)
- **Media**: Figma API access (read designs)
- **Notes**: Simpler Figma read-only integration.

### mcp-browser-screenshot / @kazuph/mcp-screenshot / mcp-screenshot-server
- **npm**: `mcp-browser-screenshot` (v1.0.0), `@kazuph/mcp-screenshot` (v1.0.4), `mcp-screenshot-server` (v1.1.3)
- **Media**: Browser/screen screenshots
- **Backend**: Playwright / native screenshot APIs
- **Output**: Base64 images or saved files
- **Notes**: Useful for capturing web page renders. `mcp-browser-screenshot` uses Playwright with navigate/click/type/eval + screenshot.

---

## 3. PDF Generation & Manipulation

### @mcp-z/mcp-pdf
- **npm**: `@mcp-z/mcp-pdf` (v2.0.7)
- **GitHub**: https://github.com/mcp-z/mcp-pdf (inferred)
- **Media**: PDF generation from layouts, text, and JSON Resume format
- **Backend**: PDFKit (offline, no API needed)
- **Output**: Files saved to disk
- **Key Tools**:
  - `pdf-document` -- generate PDF from content array
  - `pdf-resume` -- generate professional resumes from JSON Resume schema
  - `pdf-render-page` -- render PDF pages as images
  - `pdf-measure-text` -- measure text dimensions before layout
- **Params**: Extensive layout control -- two-column layouts, styling, typography, custom fonts, page sizes, sections config with LiquidJS templates
- **Data Return**: File path on disk
- **Notes**: Full emoji/Unicode support. No API key needed. Supports stdio and HTTP transports.

### pdfcrowd-mcp-pdf-export
- **npm**: `pdfcrowd-mcp-pdf-export` (v1.2.2)
- **GitHub**: https://github.com/pdfcrowd/pdfcrowd-mcp-pdf-export
- **Media**: HTML-to-PDF conversion
- **Backend**: PDFCrowd cloud service
- **Output**: PDF files saved to disk
- **Key Tools**:
  - Generate PDFs from HTML/descriptions (the LLM generates the HTML, PDFCrowd converts)
- **Params**: Output path, content description
- **Data Return**: File path on disk
- **Notes**: No API key needed for watermarked output. Professional results. Also available as Claude Code plugin. Source code stays local -- only rendered HTML sent to PDFCrowd.

### mcp-pdf
- **npm**: `mcp-pdf` (v1.1.0)
- **GitHub**: https://github.com/nitaiaharoni1/pdf-mcp
- **Media**: PDF manipulation (read, edit, merge, split, forms, signatures, security)
- **Backend**: pdf-lib (local, offline)
- **Output**: Modified PDF files saved to disk
- **Key Tools** (25+ operations):
  - **Document**: Open, create, save, get info
  - **Forms**: List/fill/flatten form fields, export form data
  - **Editing**: Add text, images, annotations, watermarks, modify metadata
  - **Pages**: Merge, split, rotate, delete, extract pages
  - **Signatures**: Visual signatures, signature fields, digital signatures
  - **Security**: Encrypt, set permissions
  - **Export**: Extract text/images, compress PDF
- **Data Return**: File paths on disk
- **Notes**: Comprehensive PDF toolkit. No API key needed.

### doc-ops-mcp
- **npm**: `doc-ops-mcp` (v0.3.8)
- **GitHub**: https://github.com/ssshuai99/doc-ops-mcp (inferred)
- **Media**: Document format conversion (PDF, DOCX, HTML, Markdown)
- **Backend**: Playwright (for HTML rendering), local converters
- **Output**: Files saved to disk (configurable `OUTPUT_DIR`)
- **Key Tools**:
  - Convert between PDF/DOCX/HTML/Markdown
  - Content rewriting (batch text replacement, regex, format adjustment)
  - PDF enhancement (watermarks, QR codes)
- **Data Return**: File paths on disk
- **Notes**: Universal document converter. Supports watermark/QR code overlay on PDFs.

### @sylphlab/tools-pdf
- **npm**: `@sylphlab/tools-pdf` (v0.7.1)
- **Media**: PDF text extraction, conversion to markdown
- **Notes**: Core library for PDF reading/extraction tools.

---

## 4. Video

### mcp-video-analyzer
- **npm**: `mcp-video-analyzer` (v0.2.4)
- **GitHub**: https://github.com/guimatheus92/mcp-video-analyzer
- **Media**: Video analysis -- transcripts, key frames, OCR, metadata
- **Backend**: yt-dlp + ffmpeg (frame extraction), Whisper (transcription fallback), Chrome (fallback)
- **Output**: Text transcripts, base64 frame images, metadata JSON
- **Key Tools**:
  - `analyze_video` -- full analysis (transcript + frames + OCR + timeline + metadata + comments)
  - `get_transcript` -- transcript only (with Whisper fallback)
  - `get_metadata` -- metadata + comments + chapters
  - `get_frames` -- scene-change detection or dense sampling (1fps)
  - `analyze_moment` -- deep dive on a time range
  - `get_frame_at` -- single frame at timestamp
- **Params**: detail level (brief/standard/detailed), fields filter, maxFrames (1-60), threshold (0.0-1.0), skipFrames, forceRefresh
- **Data Return**: Transcript text, base64 encoded frames, structured metadata JSON
- **Notes**: Supports Loom, direct video files (.mp4, .webm). Featured in awesome-mcp-servers. Most comprehensive video MCP server available. Analysis only -- does NOT generate video.

### @openmcprouter/mcp-server-ghibli-video
- **npm**: `@openmcprouter/mcp-server-ghibli-video` (v0.1.0)
- **Media**: Ghibli-style video generation
- **Notes**: Niche video generation server. Minimal documentation available.

### Replicate-based video (via mcp-replicate)
- The generic `mcp-replicate` server can run Runway, Pika, and other video models available on Replicate
- Models include: minimax/video-01, luma/ray, stability-ai/stable-video-diffusion
- **Data Return**: URLs from Replicate CDN

**Notable absence**: No dedicated MCP servers found for Runway ML, Pika, Kling, or Sora as standalone MCP integrations. These are accessible indirectly via `mcp-replicate` or `replicate-mcp` for models hosted on Replicate.

---

## 5. Audio / TTS / STT

### @kajidog/mcp-tts-voicevox
- **npm**: `@kajidog/mcp-tts-voicevox` (v0.7.2)
- **GitHub**: https://github.com/kajidog/mcp-tts-voicevox (inferred)
- **Media**: Text-to-speech via VOICEVOX engine
- **Backend**: VOICEVOX Engine (Japanese TTS, must be running locally)
- **Output**: Audio playback (server-side or client-side via MCP Apps) + WAV export
- **Key Tools**:
  - `voicevox_speak` -- text to speech with server-side playback (ffplay/afplay/aplay)
  - `speak_player` -- UI audio player embedded in chat (MCP Apps)
  - `resynthesize_player` -- update player segments
  - `get_player_state` -- read player state for AI tuning
  - `open_dictionary_ui` -- user dictionary manager
- **Params**: Text, speaker ID, speed, volume, intonation, pause length, pre/post silence
- **Data Return**: Audio playback (streaming via ffplay stdin or temp file) + WAV file export
- **Notes**: Multi-character conversations. Segment editing (speed, volume, pitch). Accent phrase editing. WAV export. Cross-platform. Works with ChatGPT, Claude Desktop. Most polished TTS MCP server.

### @angelogiacco/elevenlabs-mcp-server
- **npm**: `@angelogiacco/elevenlabs-mcp-server` (v1.0.4)
- **GitHub**: https://github.com/angelogiacco/elevenlabs-mcp-server (inferred)
- **Media**: Full ElevenLabs API (TTS, voice cloning, audio processing)
- **Backend**: ElevenLabs API
- **Output**: Varies by endpoint (audio files, voice data)
- **Key Tools**: All ElevenLabs OpenAPI endpoints exposed as MCP tools
- **Params**: Full ElevenLabs API parameters
- **Data Return**: Depends on endpoint (audio data, metadata)
- **Notes**: Auto-generated from ElevenLabs OpenAPI spec. Requires `ELEVENLABS_API_KEY`.

### elevenlabs-mcp-enhanced
- **npm**: `elevenlabs-mcp-enhanced` (v0.9.11)
- **GitHub**: https://github.com/199-biotechnologies/elevenlabs-mcp-enhanced
- **Media**: Enhanced ElevenLabs TTS + STT + conversational AI
- **Backend**: ElevenLabs API (including v3 model alpha)
- **Output**: Audio files, transcripts, conversation data
- **Key Tools**:
  - `text_to_speech` -- standard TTS
  - `text_to_dialogue` -- multi-speaker dialogue generation (v3)
  - `search_voices` -- find voices (with smart defaults)
  - Conversation history + transcript retrieval
  - Voice cloning
  - Audio transcription
- **Params**: Text, model (v1/v3), voice ID, stability, audio tags ([laughing], [crying], [piano])
- **Data Return**: Audio files, transcript text, conversation JSON
- **Notes**: Enhanced fork with v3 model support, audio tags for expressiveness, multi-speaker dialogue, auto-split long dialogues, conversation history features. Most feature-rich ElevenLabs MCP server.

### @chinchillaenterprises/mcp-elevenlabs
- **npm**: `@chinchillaenterprises/mcp-elevenlabs` (v1.0.0)
- **Media**: ElevenLabs STT with speaker diarization
- **Backend**: ElevenLabs API
- **Key Tools**: Transcription with speaker segmentation, sentiment analysis
- **Notes**: Multi-tenant. Focused on speech-to-text (not TTS).

### Replicate-based audio (via mcp-replicate)
- Whisper models for STT, MusicGen for music, Bark for TTS
- Accessible via `create_prediction` tool with appropriate model versions

**Notable absences**: No dedicated OpenAI TTS/Whisper MCP server found on npm. OpenAI's TTS and Whisper are accessible indirectly via Replicate or by building custom MCP wrappers. No dedicated Suno MCP server found.

---

## 6. Charts / Diagrams / Data Visualization

### @antv/mcp-server-chart
- **npm**: `@antv/mcp-server-chart` (v0.9.10)
- **GitHub**: https://github.com/antvis/mcp-server-chart
- **Media**: Chart and data visualization generation (26+ chart types)
- **Backend**: AntV (Alibaba visualization library)
- **Output**: Chart images (rendered server-side)
- **Key Tools** (26+ generate_* tools):
  - `generate_line_chart`, `generate_bar_chart`, `generate_column_chart`
  - `generate_pie_chart`, `generate_area_chart`, `generate_scatter_chart`
  - `generate_radar_chart`, `generate_funnel_chart`, `generate_treemap_chart`
  - `generate_sankey_chart`, `generate_boxplot_chart`, `generate_histogram_chart`
  - `generate_dual_axes_chart`, `generate_violin_chart`, `generate_venn_chart`
  - `generate_word_cloud_chart`, `generate_liquid_chart`
  - `generate_flow_diagram`, `generate_mind_map`, `generate_network_graph`
  - `generate_organization_chart`, `generate_fishbone_diagram`
  - `generate_district_map`, `generate_pin_map`, `generate_path_map`
  - `generate_spreadsheet` (pivot tables)
- **Data Return**: Chart image data (rendered visualization)
- **Notes**: Most comprehensive chart MCP server. Also available on Dify marketplace. Supports SSE/Streamable transport. Also ships a chart-visualization skill for Claude Code.

### mcp-diagram-generator
- **npm**: `mcp-diagram-generator` (v1.1.1)
- **GitHub**: https://github.com/alkaidy/mcp-diagram-generator (inferred)
- **Media**: Diagram generation in Draw.io, Mermaid, and Excalidraw formats
- **Output**: Diagram files (.drawio, .mmd, .excalidraw)
- **Key Tools**:
  - `generate_diagram` -- from structured JSON spec
- **Params**: Format (drawio/mermaid/excalidraw), elements, connections, styling (5 color schemes), nested containers (up to 10 levels)
- **Data Return**: Diagram files on disk
- **Notes**: Supports architecture diagrams, flowcharts, sequence diagrams, class diagrams, ER diagrams, mind maps, network topology.

---

## 7. 3D / Design

### mcp-3d-printer-server
- **npm**: `mcp-3d-printer-server` (v1.2.2)
- **GitHub**: https://github.com/dmontgomery40/mcp-3d-printer-server
- **Media**: 3D printing + STL manipulation + Blender bridge
- **Backend**: OctoPrint, Klipper, Duet, Repetier, Bambu Labs, Prusa Connect, Creality APIs
- **Output**: G-code files, SVG visualizations, modified STL files
- **Key Tools**:
  - **STL Manipulation**: Extend base, scale, rotate, translate, modify sections, analyze, generate SVG visualizations
  - **Printer Control**: Get status, list/upload files, start/cancel/monitor prints, set temperatures
  - **Bambu-specific**: Print .3mf files directly via MQTT, preset management
  - **Blender bridge**: `blender_mcp_edit_model` for model editing collaboration
  - **Slicing**: STL to G-code
- **Data Return**: File paths, printer status JSON, SVG visualization strings
- **Notes**: Supports 7 printer management systems. Full end-to-end workflow from STL modification to printing.

### Figma MCP Servers (see Image Processing section)
- Design asset export and manipulation via Figma API

**Notable absences**: No dedicated MCP servers found for Blender standalone (the 3D printer server has a bridge), Three.js scene generation, OpenSCAD, or SVG generation. SVG generation is partially covered by `@ricardopera/mcp-image-server` (SVG with embedded base64) and diagram generators.

---

## 8. Document Generation

### @docx-mcp/docx-mcp
- **npm**: `@docx-mcp/docx-mcp` (v0.5.0)
- **GitHub**: https://github.com/lihongjie0209/docx-mcp (inferred)
- **Media**: DOCX (Word) document creation and editing
- **Backend**: docx library (offline)
- **Output**: .docx files saved to disk
- **Key Tools**:
  - Create DOCX from JSON schema
  - Edit existing DOCX documents
  - Save to disk
- **Content Blocks**: Headings (6 levels), paragraphs, tables, images (URL/local/base64), code blocks (180+ languages with syntax highlighting), lists (ordered/unordered with nesting), page breaks, horizontal rules, blockquotes, info boxes, text boxes
- **Features**: Headers/footers (default/first/even pages), page numbering, metadata, rich text formatting (bold, italic, underline, superscript, subscript, colors, fonts, highlights)
- **Data Return**: File path on disk
- **Notes**: Most comprehensive DOCX MCP server. Full JSON schema validation. Image support with URL download fallback.

### mcp-powerpoint
- **npm**: `mcp-powerpoint` (v0.1.3)
- **GitHub**: https://github.com/islem-zaraa/mcp-powerpoint
- **Media**: PowerPoint presentation creation and manipulation
- **Backend**: pptxgenjs (offline)
- **Output**: .pptx files saved to disk
- **Key Tools**:
  - `mcp_powerpoint_create_presentation` -- create new .pptx
  - `mcp_powerpoint_add_slide` -- add slides with title/content
  - `mcp_powerpoint_get_slides` -- read slide info
  - `mcp_powerpoint_export_to_pdf` -- export to PDF (simulated)
  - `mcp_powerpoint_read_presentation` -- read metadata/structure
- **Data Return**: File path on disk
- **Notes**: Basic functionality. PDF export is simulated in current version. Limited slide editing for existing presentations.

### @negokaz/excel-mcp-server
- **npm**: `@negokaz/excel-mcp-server` (v0.12.0)
- **GitHub**: https://github.com/negokaz/excel-mcp-server
- **Media**: Excel spreadsheet read/write (.xlsx, .xlsm, .xltx, .xltm)
- **Backend**: ExcelJS (cross-platform) + COM interface (Windows)
- **Output**: Modified .xlsx files on disk
- **Key Tools**:
  - `excel_describe_sheets` -- list sheets
  - `excel_read_sheet` -- read with pagination
  - `excel_write_to_sheet` -- write values/formulas
  - `excel_create_table` -- create tables
  - `excel_copy_sheet` -- copy sheets
  - `excel_screen_capture` -- screenshot of sheet (Windows only, returns base64 image)
- **Data Return**: Cell data as text, file on disk, base64 screenshot (Windows)
- **Notes**: Most polished Excel MCP server. Pagination support for large sheets. Windows has live editing via COM + screen capture.

### mcp-excel-controller-pro
- **npm**: `mcp-excel-controller-pro` (v1.5.2)
- **Media**: Excel file operations (requires MS Excel installed)
- **Backend**: COM interface (Windows-centric)
- **Key Tools**: read_excel, bulk_update_excel, add_sheet, rename_sheet, delete_sheet, list_open_excel_files, close_excel_file
- **Notes**: Windows-focused. Can modify files while Excel has them open. Backup functionality.

### @piotr-agier/google-drive-mcp
- **npm**: `@piotr-agier/google-drive-mcp` (v1.7.5)
- **GitHub**: https://github.com/piotr-agier/google-drive-mcp (inferred)
- **Media**: Google Drive, Docs, Sheets, Slides, Calendar
- **Backend**: Google APIs (Drive, Docs, Sheets, Slides, Calendar)
- **Output**: Google Workspace documents (cloud)
- **Key Tools**:
  - File management (create, update, delete, rename, move, copy, upload, download)
  - Google Docs editing (text insertion/deletion, tables, images, comments, formatting)
  - Google Sheets operations
  - Google Slides creation
  - Google Calendar management
  - Shared Drives support
- **Data Return**: Google Drive file IDs, document content, file download
- **Notes**: Most comprehensive Google Workspace MCP server. OAuth 2.0 authentication. Supports folder navigation with path syntax.

---

## 9. Browser Automation (Screenshot/PDF capable)

### @modelcontextprotocol/server-puppeteer (Official)
- **npm**: `@modelcontextprotocol/server-puppeteer`
- **GitHub**: https://github.com/modelcontextprotocol/servers (monorepo)
- **Media**: Browser screenshots, page interaction
- **Backend**: Puppeteer (headless Chrome)
- **Output**: Screenshots as base64 or binary PNG
- **Key Tools**:
  - `puppeteer_navigate` -- navigate to URL
  - `puppeteer_screenshot` -- capture page/element screenshots (base64 or binary)
  - `puppeteer_click`, `puppeteer_fill`, `puppeteer_select`, `puppeteer_hover`
  - `puppeteer_evaluate` -- execute JavaScript
- **Resources**: Console logs (`console://logs`), screenshots (`screenshot://<name>`)
- **Data Return**: Base64 encoded PNG or binary image content
- **Notes**: Official MCP reference server. Can be used for HTML-to-screenshot pipeline. Docker support available.

### @playwright/mcp
- **npm**: `@playwright/mcp`
- **Media**: Browser automation + screenshots
- **Backend**: Playwright (Chromium, Firefox, WebKit)
- **Notes**: Official Playwright MCP server. Similar capabilities to Puppeteer server but with multi-browser support.

### chrome-local-mcp
- **npm**: `chrome-local-mcp` (v1.3.0)
- **Media**: Chrome automation + screenshots
- **Backend**: Puppeteer
- **Notes**: Local Chrome browser automation optimized for Claude Code.

---

## Summary: How Servers Return Binary Content

| Pattern | Servers | Mechanism |
|---------|---------|-----------|
| **File on disk** | comfyui-mcp, mcp-image, @ricardopera/mcp-image-server, mcp-fal-ai-image, @mcp-z/mcp-pdf, pdfcrowd-mcp-pdf-export, mcp-pdf, @docx-mcp/docx-mcp, mcp-powerpoint, @negokaz/excel-mcp-server, mcp-diagram-generator | Save file, return path string |
| **URL** | @iflow-mcp/mcp-replicate-flux, mcp-replicate, replicate-mcp, @iflow-mcp/inhiblabcore-mcp-image-compression | Return HTTP(S) URL to hosted content |
| **Base64 in response** | mcp-image-extractor, @modelcontextprotocol/server-puppeteer, mcp-browser-screenshot, @kazuph/mcp-screenshot | Return base64 string as text content |
| **Audio playback** | @kajidog/mcp-tts-voicevox | Play audio server-side or via MCP Apps UI player |
| **Cloud document** | @piotr-agier/google-drive-mcp | Create/modify documents in Google Drive |
| **Chart render** | @antv/mcp-server-chart | Server-side rendered image returned |

---

## Notable Gaps (as of March 2026)

| Category | Missing | Notes |
|----------|---------|-------|
| **Image Gen** | No dedicated Midjourney MCP server | Midjourney lacks official API; Discord-based wrappers exist but not as MCP |
| **Image Gen** | No dedicated Stability AI MCP server | Can use Replicate or fal.ai gateways |
| **Image Processing** | No dedicated Cloudinary MCP server | Cloudinary has REST API but no MCP wrapper found |
| **Image Processing** | No dedicated Sharp/ImageMagick MCP server | No npm MCP server wrapping Sharp or ImageMagick for resize/crop/transform |
| **Image Processing** | No dedicated background removal MCP server | Available via Replicate models |
| **Video Gen** | No dedicated Runway/Pika/Sora/Kling MCP servers | Accessible indirectly via Replicate for hosted models |
| **Audio** | No dedicated OpenAI TTS/Whisper MCP server | ElevenLabs alternatives exist; Whisper via Replicate |
| **Audio** | No dedicated Suno music generation MCP server | Suno API exists but no MCP wrapper |
| **3D** | No Blender standalone MCP server | 3D printer server has a bridge; no full Blender MCP |
| **3D** | No Three.js / WebGL scene generation MCP server | -- |
| **Design** | No Canva MCP server | -- |
| **Documents** | No dedicated Markdown-to-PDF MCP server | doc-ops-mcp covers this via conversion |
| **Documents** | No WeasyPrint MCP server | -- |

---

## Methodology

- **Sources searched**: npm registry (`npm search`), npm package READMEs (`npm view ... readme`), GitHub, smithery.ai, mcp.so, glama.ai (references found in package metadata)
- **Packages analyzed**: 40+ MCP server packages examined
- **Date**: March 17, 2026
- **Confidence**: High for npm-published servers; Medium for Python-only servers (pip search disabled, GitHub search limited to what was referenced)
- **Limitation**: Python-only MCP servers (published only on PyPI or GitHub without npm) may be underrepresented. The ElevenLabs official Python MCP server, for example, exists but was found via its npm-wrapped fork.
