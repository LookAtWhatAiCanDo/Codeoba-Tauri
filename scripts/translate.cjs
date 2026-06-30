const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');

const apiKey = process.env.GEMINI_API_KEY;

if (!apiKey) {
  console.error("Error: GEMINI_API_KEY environment variable is not defined.");
  process.exit(1);
}

// Flat map of a nested object into dot notation
function getFlatKeys(obj, prefix = '') {
  let keys = {};
  for (const k in obj) {
    const newPrefix = prefix ? `${prefix}.${k}` : k;
    if (typeof obj[k] === 'object' && obj[k] !== null && !Array.isArray(obj[k])) {
      Object.assign(keys, getFlatKeys(obj[k], newPrefix));
    } else {
      keys[newPrefix] = obj[k];
    }
  }
  return keys;
}

// Set nested value in object by path
function setNestedValue(obj, pathStr, value) {
  const parts = pathStr.split('.');
  let current = obj;
  for (let i = 0; i < parts.length - 1; i++) {
    const part = parts[i];
    if (!(part in current) || typeof current[part] !== 'object' || current[part] === null) {
      current[part] = {};
    }
    current = current[part];
  }
  current[parts[parts.length - 1]] = value;
}

// Ask Gemini to reconcile a batch of differences
async function reconcileBatch(batch, targetLang, attempt = 1) {
  const langNames = {
    "es": "Spanish",
    "fr": "French",
    "de": "German",
    "ja": "Japanese",
    "zh": "Simplified Chinese",
    "zh-TW": "Traditional Chinese",
    "pt": "Portuguese",
    "it": "Italian",
    "ko": "Korean",
    "nl": "Dutch",
    "ar": "Arabic",
    "ru": "Russian"
  };

  const targetName = langNames[targetLang] || targetLang;

  const prompt = `You are a professional software localizer. You are reconciling differences in translation keys for target language "${targetName}" (code: "${targetLang}").

We have two versions of target translations:
1. Version A (HEAD / Previous Commit): Generally high-quality, has correct capitalization and polite verb tones.
2. Version B (Working Copy / LLM Draft): Updated by a script, corrects typos (like changing "Codeova" to "Codeoba") and translates missing words, but sometimes introduces incorrect capitalization or informal verb tones.

Reconciliation Guidelines:
1. Capitalization: Prefer Version A's sentence case (only capitalize the first word and proper nouns) for buttons, titles, headers, and UI labels. Do not use Version B's English-style title-casing (e.g. do not capitalize every word).
2. Formality: Prefer Version A's polite/formal address tones (e.g., formal 'usted' in Spanish, 'vous' in French, polite forms in Japanese/Korean/German) over Version B's informal tones.
3. Brand & Typos: Keep Version B's spelling corrections and typo fixes (e.g., spelling the product name "Codeoba" instead of "Codeova", and resolving character typos).
4. Variable Safety: Placeholder variables (like "{count}", "{version}", "{progress}", "{error}") must match the English source exactly and remain completely untranslated and unmodified.
5. Choose the translation that is more natural, concise, and standard for modern software UI.
6. If Version A and Version B are both empty (or missing), this is a brand new key. Translate the English original directly using the capitalization and formality guidelines above.
7. Use the "references" object (which contains existing translations for this key in other languages) to help understand context, resolve ambiguities (e.g., parts of speech, noun vs. verb, register), and ensure consistency across locales.

Input JSON format:
{
  "key.path": {
    "english": "English Original",
    "versionA": "HEAD translation",
    "versionB": "Working Copy translation",
    "references": {
      "lang_code": "Existing translation in other language"
    }
  }
}

Output JSON format (return a single JSON object mapping keys to your chosen reconciled translation):
{
  "key.path": "Final Reconciled Translation"
}

Output your response as a valid JSON object. Do not include markdown code block formatting (like \`\`\`json).

Input JSON:
${JSON.stringify(batch, null, 2)}`;

  const url = `https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent?key=${apiKey}`;

  const response = await fetch(url, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({
      contents: [{
        parts: [{
          text: prompt
        }]
      }],
      generationConfig: {
        responseMimeType: "application/json"
      }
    })
  });

  if (!response.ok) {
    if (response.status === 429 && attempt <= 3) {
      console.warn(`  Rate limit hit (429). Waiting 30 seconds before retry attempt ${attempt}/3...`);
      await new Promise(r => setTimeout(r, 30000));
      return reconcileBatch(batch, targetLang, attempt + 1);
    }
    const errText = await response.text();
    throw new Error(`Gemini API response error (${response.status}): ${errText}`);
  }

  const result = await response.json();
  const text = result.candidates[0].content.parts[0].text;
  
  try {
    return JSON.parse(text.trim());
  } catch (err) {
    console.error("Failed to parse LLM response as JSON. Raw response was:", text);
    throw err;
  }
}

const localesDir = path.join(__dirname, '../src/i18n/locales');
const enPath = path.join(localesDir, 'en.json');

if (!fs.existsSync(enPath)) {
  console.error(`Error: en.json not found at ${enPath}`);
  process.exit(1);
}

const enJson = JSON.parse(fs.readFileSync(enPath, 'utf8'));
const enFlat = getFlatKeys(enJson);

const targetLanguages = ["es", "fr", "de", "ja", "zh", "zh-TW", "pt", "it", "ko", "nl", "ar", "ru"];

async function run() {
  const dryRun = process.argv.includes('--dry-run') || process.argv.includes('-d');
  
  let totalStrings = 0;
  let totalApiCalls = 0;

  if (dryRun) {
    console.log("=== DRY RUN MODE: No API calls will be made, no files will be written ===\n");
  } else {
    console.log("Starting translation reconciliation process...");
  }

  // Pre-load HEAD versions of all target languages to use as references
  console.log("Loading translation baselines from HEAD/files...");
  const headFlats = {};
  for (const lang of targetLanguages) {
    const file = `${lang}.json`;
    const filePath = path.join(localesDir, file);
    let headJson = {};
    try {
      const headContent = execSync(`git show HEAD:src/i18n/locales/${file}`, { encoding: 'utf8', stdio: ['pipe', 'pipe', 'ignore'] });
      headJson = JSON.parse(headContent);
    } catch (err) {
      if (fs.existsSync(filePath)) {
        try {
          headJson = JSON.parse(fs.readFileSync(filePath, 'utf8'));
        } catch (_) {}
      }
    }
    headFlats[lang] = getFlatKeys(headJson);
  }

  for (const lang of targetLanguages) {
    const file = `${lang}.json`;
    const filePath = path.join(localesDir, file);
    
    if (!fs.existsSync(filePath)) {
      console.log(`- ${lang}: File not found, skipping.`);
      continue;
    }

    const workingJson = JSON.parse(fs.readFileSync(filePath, 'utf8'));
    const workingFlat = getFlatKeys(workingJson);

    // Get pre-loaded baseline version
    const headFlat = headFlats[lang] || {};

    // Find difference between HEAD and working copy, or completely missing translations
    const diffKeys = [];
    const payload = {};

    for (const key of Object.keys(enFlat)) {
      const headVal = headFlat[key] || '';
      const workVal = workingFlat[key] || '';

      // If they differ, or if the working copy is missing/empty, we must process it
      if (headVal !== workVal || !workVal) {
        diffKeys.push(key);

        // Gather references from other languages (only where they exist and aren't empty)
        const referenceTranslations = {};
        for (const otherLang of targetLanguages) {
          if (otherLang !== lang) {
            const refVal = headFlats[otherLang]?.[key] || '';
            if (refVal) {
              referenceTranslations[otherLang] = refVal;
            }
          }
        }

        payload[key] = {
          english: enFlat[key],
          versionA: headVal,
          versionB: workVal,
          ...(Object.keys(referenceTranslations).length > 0 ? { references: referenceTranslations } : {})
        };
      }
    }

    if (diffKeys.length === 0) {
      console.log(`- ${lang}: Reconciled (No differences found).`);
      continue;
    }

    // Increment trackers
    totalStrings += diffKeys.length;
    totalApiCalls += 1;

    if (dryRun) {
      console.log(`- ${lang}: Found ${diffKeys.length} differences to reconcile:`);
      for (const key of diffKeys) {
        console.log(`  [Key]  "${key}"`);
        console.log(`    English:   "${payload[key].english}"`);
        console.log(`    Version A: "${payload[key].versionA}"`);
        console.log(`    Version B: "${payload[key].versionB}"`);
      }
      console.log("");
      continue;
    }

    console.log(`- ${lang}: Found ${diffKeys.length} differences. Reconciling with Gemini...`);

    try {
      const reconciledValues = await reconcileBatch(payload, lang);
      
      // Update workingJson with reconciled values
      for (const key of diffKeys) {
        if (reconciledValues && reconciledValues[key] !== undefined) {
          setNestedValue(workingJson, key, reconciledValues[key]);
        } else {
          console.warn(`    Warning: Key "${key}" was not returned in reconciled response, keeping Version B.`);
        }
      }

      // Write reconciled file back to disk
      fs.writeFileSync(filePath, JSON.stringify(workingJson, null, 2) + '\n', 'utf8');
      console.log(`  Successfully reconciled and saved ${file}`);

    } catch (err) {
      console.error(`  Error reconciling ${lang}:`, err.message);
      process.exit(1);
    }

    // Small rate limiting delay between languages
    await new Promise(r => setTimeout(r, 2000));
  }

  if (dryRun) {
    console.log("=======================================================================");
    console.log(`Dry run completed.`);
    console.log(`Metrics: Would have processed ${totalStrings} strings across ${totalApiCalls} API calls.`);
    console.log("Run without --dry-run or -d to apply the reconciliation.");
    console.log("=======================================================================");
  } else {
    console.log("\nReconciliation completed successfully!");
    console.log(`Metrics: Processed ${totalStrings} strings across ${totalApiCalls} API calls.`);
  }
}

run().catch(console.error);
