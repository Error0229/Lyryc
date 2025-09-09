import { isLikelyJapanese, isLikelyChinese, isJapanese, hasKanji } from './japaneseDetection';
import { kuroshiroService } from '../services/kuroshiroService';

// Test data
const testCases = [
  {
    text: "私は学生です。今日は良い天気ですね。", // Japanese with hiragana + kanji
    expected: { japanese: true, chinese: false, hasJapanese: true, hasKanji: true }
  },
  {
    text: "这是中文句子。我们今天学习汉语。", // Chinese text (no kana)
    expected: { japanese: false, chinese: true, hasJapanese: false, hasKanji: true }
  },
  {
    text: "こんにちは、元気ですか？", // Japanese (hiragana only)
    expected: { japanese: true, chinese: false, hasJapanese: true, hasKanji: false }
  },
  {
    text: "カタカナで書いています", // Japanese (katakana + kanji)
    expected: { japanese: true, chinese: false, hasJapanese: true, hasKanji: true }
  },
  {
    text: "Hello, how are you today?", // English
    expected: { japanese: false, chinese: false, hasJapanese: false, hasKanji: false }
  }
];

export function testLanguageDetection() {
  console.log('🧪 Testing improved language detection...');
  
  testCases.forEach((testCase, index) => {
    const results = {
      japanese: isLikelyJapanese(testCase.text),
      chinese: isLikelyChinese(testCase.text),
      hasJapanese: isJapanese(testCase.text),
      hasKanji: hasKanji(testCase.text)
    };
    
    const passed = 
      results.japanese === testCase.expected.japanese &&
      results.chinese === testCase.expected.chinese &&
      results.hasJapanese === testCase.expected.hasJapanese &&
      results.hasKanji === testCase.expected.hasKanji;
    
    console.log(`Test ${index + 1}: ${passed ? '✅' : '❌'} "${testCase.text}"`);
    console.log(`  Expected: ${JSON.stringify(testCase.expected)}`);
    console.log(`  Got:      ${JSON.stringify(results)}`);
    console.log('');
  });
  
  console.log('Language detection tests completed!');
}

export async function testJapaneseProcessing() {
  console.log('🧪 Testing Japanese processing service...');
  
  const testTexts = [
    "今日は良い天気です", // Japanese with kanji
    "私は学生です", // Japanese with kanji
    "Hello World", // English
    "こんにちは", // Japanese hiragana only
  ];
  
  for (const text of testTexts) {
    console.log(`\n📝 Testing: "${text}"`);
    
    try {
      const hiragana = await kuroshiroService.convertToHiragana(text);
      const furigana = await kuroshiroService.convertToFurigana(text);
      
      console.log(`  ✅ Hiragana: ${hiragana}`);
      console.log(`  ✅ Furigana: ${furigana}`);
    } catch (error) {
      console.log(`  ❌ Error: ${error}`);
    }
  }
  
  console.log('\n🎉 Japanese processing tests completed!');
}