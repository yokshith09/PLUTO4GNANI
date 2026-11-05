# T-009 / T-LLM-01 — Gemini and Groq JSON check

**Date:** 4 Nov 2026 · **Run by:** D2 · **Machine:** Windows, `cargo run -p pluto-intel --example llm_check` at `22be31a`
**Data:** five synthetic meeting excerpts (English, Hinglish; AS-1 style), same extraction schema for both models. No real data.

## Result: pass (5 of 5 valid on both)

| Provider | Model | Schema-valid | Quotes found in the source | Latency (5 runs) |
|---|---|---|---|---|
| Gemini | `gemini-2.5-flash` | 5 / 5 (twice: 10 / 10 across two runs) | 5 / 5 each run | 2.0–5.8 s, median ≈ 2.6 s |
| Groq | `openai/gpt-oss-120b` | 5 / 5 | 5 / 5 | 0.7–1.8 s, median ≈ 1.3 s |

- Items found per excerpt: 2–4 for both.
- `llama-3.3-70b-versatile` returned **404 "does not exist or you do not have access"** on the team Groq account for all 5 calls, so it is not used. `openai/gpt-oss-120b` is the Groq default (setting `llm.groq_model`).

## Limits observed (from Groq's response headers, `openai/gpt-oss-120b`)
- 1,000 requests per day; 8,000 tokens per minute. Each test call used about 400–600 tokens (the tokens-remaining header fell from 8,000 to about 5,300 over 5 calls).
- Gemini free-tier limits: **to be copied from AI Studio by the account owner** (requests per minute and per day, tokens per minute). Not visible from the API response.

## What this means for the plan
- Meeting extraction and recall (Gemini first) take about 2–6 s per call: fine for a review screen, with a "Reading the transcript…" state.
- Voice commands (Groq first) answer in about 1–2 s, within the 1.5 s p50 target only for short prompts; measure properly in T-LAT-03.
- 8,000 tokens a minute is the tight limit: about 15 voice commands a minute. A heavy burst gets `429` and falls back to Gemini (tested: T-FH-07).
- A model name that stops working is a settings change, not a release.

## Extraction on the AS-1 excerpt (first reading for T-ML-02)
`cargo run -p pluto-intel --example extract_smoke` on the 7-line AS-1 fixture transcript, meeting date Monday 2 Nov 2026, Gemini (4 Nov 2026):

| Found | Owner | Due (said, resolved) | Matches ground truth |
|---|---|---|---|
| Action: complete the pricing copy | Ravi | "Thursday tak" -> 2026-11-05 | Yes, exactly |
| Action: confirm the vendor contract with legal | Meena | "Monday tak" -> 2026-11-09 | Yes, exactly |
| Action: the pricing page should be live | none | "Friday tak" -> 2026-11-06 | Extra: a goal Asha stated, with no owner; flagged by the "no owner" warning |
| Decision: start with the annual plan | | | Yes |
| Question: who owns the vendor contract | | | Yes |
| Risk: launch may be missed if legal is late | | | Yes |
| Small talk ("chai break") | | | Correctly left out |

- Both real action items: right owner, right date. All six quotes are exact lines from the transcript.
- One extra item (precision 2 of 3 on action items; recall 2 of 2). The review screen is where a person drops or assigns it, and the warning points at it.
- This is an excerpt (the fixture holds 7 of the 21 AS-1 lines), so it is a first reading, not the T-ML-02 number. The full figure needs the real recording through Gnani (T-201).

## Still to do
- Copy the Gemini free-tier limits into this file.
- T-ML-02 on the full AS-1 to AS-3 transcripts, and T-ML-04 (routing on AS-4) with these models.
