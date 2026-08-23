// Turns a raw dictated sentence into what actually belongs in the text field.
// Windows' own recognizer already places some punctuation itself, so this is
// a mop-up pass: it only replaces command words the recognizer left as plain
// text, never touches a character already sitting in the sentence, and only
// capitalizes a word right after a mark IT just inserted.
//
// Loaded as a plain script (like floatShared.js), so everything here is a
// bare function/const in module scope. The guarded export at the bottom
// makes it directly testable from Node without affecting the window build.

const VOICE_PUNCT = {
  en: { "period": ".", "comma": ",", "question mark": "?", "exclamation mark": "!", "exclamation point": "!", "colon": ":", "semicolon": ";", "hyphen": "-", "dash": "–", "slash": "/", "quote": "\"", "open paren": "(", "close paren": ")", "percent": "%" },
  de: { "punkt": ".", "komma": ",", "fragezeichen": "?", "ausrufezeichen": "!", "doppelpunkt": ":", "semikolon": ";", "bindestrich": "-", "gedankenstrich": "–", "schrägstrich": "/", "anführungszeichen": "\"", "klammer auf": "(", "klammer zu": ")", "prozent": "%" },
  es: { "punto": ".", "coma": ",", "signo de interrogación": "?", "signo de exclamación": "!", "dos puntos": ":", "punto y coma": ";", "guion": "-", "raya": "–", "barra": "/", "comillas": "\"", "abrir paréntesis": "(", "cerrar paréntesis": ")", "por ciento": "%" },
  it: { "punto": ".", "virgola": ",", "punto interrogativo": "?", "punto esclamativo": "!", "due punti": ":", "punto e virgola": ";", "trattino": "-", "trattino lungo": "–", "barra": "/", "virgolette": "\"", "aperta parentesi": "(", "chiusa parentesi": ")", "per cento": "%" },
  pt: { "ponto": ".", "vírgula": ",", "ponto de interrogação": "?", "ponto de exclamação": "!", "dois pontos": ":", "ponto e vírgula": ";", "hífen": "-", "travessão": "–", "barra": "/", "aspas": "\"", "abre parênteses": "(", "fecha parênteses": ")", "por cento": "%" },
  pl: { "kropka": ".", "przecinek": ",", "znak zapytania": "?", "wykrzyknik": "!", "dwukropek": ":", "średnik": ";", "łącznik": "-", "myślnik": "–", "ukośnik": "/", "cudzysłów": "\"", "otwórz nawias": "(", "zamknij nawias": ")", "procent": "%" },
  zh: { "句号": ".", "逗号": ",", "问号": "?", "感叹号": "!", "冒号": ":", "分号": ";", "连字符": "-", "破折号": "–", "斜杠": "/", "引号": "\"", "左括号": "(", "右括号": ")", "百分号": "%" },
  ja: { "まる": ".", "てん": ",", "クエスチョンマーク": "?", "びっくりマーク": "!", "コロン": ":", "セミコロン": ";", "ハイフン": "-", "ダッシュ": "–", "スラッシュ": "/", "引用符": "\"", "かっこ": "(", "かっことじ": ")", "パーセント": "%" },
  nl: { "punt": ".", "komma": ",", "vraagteken": "?", "uitroepteken": "!", "dubbele punt": ":", "puntkomma": ";", "koppelteken": "-", "streepje": "–", "schuine streep": "/", "aanhalingsteken": "\"", "haakje openen": "(", "haakje sluiten": ")", "procent": "%" },
  tr: { "nokta": ".", "virgül": ",", "soru işareti": "?", "ünlem işareti": "!", "iki nokta": ":", "noktalı virgül": ";", "kısa çizgi": "-", "uzun çizgi": "–", "eğik çizgi": "/", "tırnak işareti": "\"", "parantez aç": "(", "parantez kapa": ")", "yüzde": "%" },
  ro: { "punct": ".", "virgulă": ",", "semn de întrebare": "?", "semn de exclamare": "!", "două puncte": ":", "punct și virgulă": ";", "cratimă": "-", "linie de pauză": "–", "bară oblică": "/", "ghilimele": "\"", "paranteză deschisă": "(", "paranteză închisă": ")", "procent": "%" },
};

const VOICE_FUNCS = {
  en: { "new line": "\n", "next line": "\n", "new paragraph": "\n\n", "tab": "\t" },
  de: { "neue zeile": "\n", "nächste zeile": "\n", "neuer absatz": "\n\n", "tabulator": "\t" },
  es: { "nueva línea": "\n", "nuevo párrafo": "\n\n", "tabulador": "\t" },
  it: { "nuova riga": "\n", "nuovo paragrafo": "\n\n", "tabulatore": "\t" },
  pt: { "nova linha": "\n", "novo parágrafo": "\n\n", "tabulação": "\t" },
  pl: { "nowa linia": "\n", "nowy akapit": "\n\n", "tabulator": "\t" },
  zh: { "换行": "\n", "新段落": "\n\n", "制表符": "\t" },
  ja: { "改行": "\n", "新しい段落": "\n\n", "タブ": "\t" },
  nl: { "nieuwe regel": "\n", "nieuwe alinea": "\n\n", "tab": "\t" },
  tr: { "yeni satır": "\n", "yeni paragraf": "\n\n", "sekme": "\t" },
  ro: { "rând nou": "\n", "paragraf nou": "\n\n", "tab": "\t" },
};

const VOICE_LITERAL_WORD = { en: "literal", de: "wörtlich", es: "literal", it: "letterale", pt: "literal", pl: "dosłownie", zh: "字面量", ja: "文字通り", nl: "letterlijk", tr: "harfi harfine", ro: "literal" };

const SENTENCE_END = new Set([".", "!", "?"]);

function voiceCommandIndex(lang) {
  const punct = VOICE_PUNCT[lang] || VOICE_PUNCT.en;
  const funcs = VOICE_FUNCS[lang] || VOICE_FUNCS.en;
  const map = new Map();
  let maxWords = 1;
  for (const [phrase, value] of Object.entries(punct)) {
    map.set(phrase, { type: "punct", value });
    maxWords = Math.max(maxWords, phrase.split(" ").length);
  }
  for (const [phrase, value] of Object.entries(funcs)) {
    map.set(phrase, { type: "func", value });
    maxWords = Math.max(maxWords, phrase.split(" ").length);
  }
  return { map, maxWords };
}

function voiceEmitWord(word, capNext, outSoFar) {
  // No space right after our own newline/tab, or after an opening bracket we
  // just inserted -- "(" should hug the word that follows it.
  const needsSpace = outSoFar.length > 0 && !/[\n\t(]$/.test(outSoFar);
  const w = capNext ? word.charAt(0).toUpperCase() + word.slice(1) : word;
  return (needsSpace ? " " : "") + w;
}

// zh/ja carry no spaces between words at all -- splitting on whitespace would
// hand the whole sentence back as one unmatched token, so command phrases are
// never recognized. Scanned by raw character position instead. Capitalization
// does not apply to either script, so autoCaps is simply not consulted here.
const CJK_LANGS = new Set(["zh", "ja"]);
function applyVoiceCommandsCjk(text, commands, literalWord) {
  let maxLen = literalWord.length;
  for (const phrase of commands.keys()) maxLen = Math.max(maxLen, phrase.length);
  let out = "";
  let forceLiteral = false;
  for (let i = 0; i < text.length; ) {
    if (forceLiteral) {
      out += text[i];
      forceLiteral = false;
      i++;
      continue;
    }
    let matched = false;
    for (let len = Math.min(maxLen, text.length - i); len >= 1; len--) {
      const chunk = text.slice(i, i + len);
      if (chunk === literalWord) {
        i += len;
        // Nothing follows in this segment to escape -- emit the word itself
        // rather than silently dropping it (same rule as the word-based path).
        if (i < text.length) forceLiteral = true; else out += chunk;
        matched = true;
        break;
      }
      if (commands.has(chunk)) {
        out += commands.get(chunk).value;
        i += len;
        matched = true;
        break;
      }
    }
    if (!matched) { out += text[i]; i++; }
  }
  return out;
}

// opts: { punct: bool (voicePunct), autoCaps: bool (voiceAutoCaps), lang }
function applyVoiceCommands(text, opts) {
  const o = opts || {};
  const lang = o.lang || "en";
  if (!o.punct || !text) return text;
  const literalWord = (VOICE_LITERAL_WORD[lang] || VOICE_LITERAL_WORD.en).toLowerCase();
  const { map: commands, maxWords } = voiceCommandIndex(lang);
  if (CJK_LANGS.has(lang)) return applyVoiceCommandsCjk(text, commands, literalWord);
  const tokens = text.split(/\s+/).filter(Boolean);
  let out = "";
  let capNext = false;
  let forceLiteral = false;
  for (let i = 0; i < tokens.length; i++) {
    if (forceLiteral) {
      out += voiceEmitWord(tokens[i], capNext, out);
      capNext = false;
      forceLiteral = false;
      continue;
    }
    if (tokens[i].toLowerCase() === literalWord) {
      // Nothing follows in this segment to escape (the word landed right at
      // a recognizer utterance boundary) -- emit it as ordinary text instead
      // of silently discarding the word the user actually said.
      if (i === tokens.length - 1) {
        out += voiceEmitWord(tokens[i], capNext, out);
        capNext = false;
        continue;
      }
      forceLiteral = true;
      continue;
    }
    let matched = null;
    let consumed = 1;
    for (let w = Math.min(maxWords, tokens.length - i); w >= 1; w--) {
      const phrase = tokens.slice(i, i + w).join(" ").toLowerCase();
      if (commands.has(phrase)) { matched = commands.get(phrase); consumed = w; break; }
    }
    if (matched) {
      i += consumed - 1;
      out += matched.value;
      if (o.autoCaps && (SENTENCE_END.has(matched.value) || matched.value === "\n\n")) capNext = true;
      continue;
    }
    out += voiceEmitWord(tokens[i], capNext, out);
    capNext = false;
  }
  return out;
}

if (typeof module !== "undefined") module.exports = { applyVoiceCommands, VOICE_PUNCT, VOICE_FUNCS, VOICE_LITERAL_WORD };
