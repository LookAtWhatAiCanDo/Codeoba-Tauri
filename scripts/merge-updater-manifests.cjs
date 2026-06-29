const fs = require('fs');
const path = require('path');

const artifactsDir = process.argv[2] || path.join(__dirname, '../artifacts');
const targetTag = process.argv[3] || process.env.GITHUB_REF_NAME || 'dev-release';

// Resolve release tag (use 'dev-release' for non-version refs like 'main')
const releaseTag = (targetTag.startsWith('v') && targetTag !== 'v*') ? targetTag : 'dev-release';

// Extract version from tag (e.g. "v0.2.0-43" -> "0.2.0-43", "v0.2.0" -> "0.2.0")
const version = releaseTag.startsWith('v') ? releaseTag.slice(1) : releaseTag;

console.log(`Generating updater manifest from artifacts at: ${artifactsDir}`);
console.log(`Using release tag for URLs: ${releaseTag}`);
console.log(`Using version: ${version}`);

if (!fs.existsSync(artifactsDir)) {
  console.error(`Error: Artifacts directory not found at ${artifactsDir}`);
  process.exit(1);
}

const mergedManifest = {
  version: version,
  pub_date: new Date().toISOString(),
  platforms: {}
};

// Recursively find all files in directory
function getAllFiles(dir) {
  let results = [];
  if (!fs.existsSync(dir)) return results;
  const list = fs.readdirSync(dir);
  for (const file of list) {
    const filePath = path.join(dir, file);
    const stat = fs.statSync(filePath);
    if (stat && stat.isDirectory()) {
      results = results.concat(getAllFiles(filePath));
    } else {
      results.push(filePath);
    }
  }
  return results;
}

const allFiles = getAllFiles(artifactsDir);

// Helper to find a file pair (installer + .sig file)
function findFilePair(ext, substring) {
  const file = allFiles.find(f => {
    const name = path.basename(f);
    const matchesExt = name.endsWith(ext);
    const matchesSub = substring ? name.toLowerCase().includes(substring.toLowerCase()) : true;
    return matchesExt && matchesSub;
  });
  if (!file) return null;
  const sigFile = allFiles.find(f => f === file + '.sig');
  return { file, sigFile };
}

// 1. Find macOS artifacts
const macApp = findFilePair('.app.tar.gz');
if (macApp && macApp.sigFile) {
  const filename = path.basename(macApp.file);
  const signature = fs.readFileSync(macApp.sigFile, 'utf8').trim();
  const url = `https://github.com/LookAtWhatAiCanDo/Codeoba/releases/download/${releaseTag}/${filename}`;
  
  mergedManifest.platforms['darwin-aarch64'] = { signature, url };
  mergedManifest.platforms['darwin-x86_64'] = { signature, url };
  console.log(`✅ Added macOS (darwin-aarch64, darwin-x86_64) update target: ${filename}`);
} else {
  console.log('⚠️ No macOS (.app.tar.gz) and signature (.app.tar.gz.sig) pair found.');
}

// 2. Find Windows x64 artifacts
const winX64 = findFilePair('.msi', 'x64');
if (winX64 && winX64.sigFile) {
  const filename = path.basename(winX64.file);
  const signature = fs.readFileSync(winX64.sigFile, 'utf8').trim();
  const url = `https://github.com/LookAtWhatAiCanDo/Codeoba/releases/download/${releaseTag}/${filename}`;
  
  mergedManifest.platforms['windows-x86_64'] = { signature, url };
  console.log(`✅ Added Windows x64 (windows-x86_64) update target: ${filename}`);
} else {
  console.log('⚠️ No Windows x64 (.msi) and signature (.msi.sig) pair found.');
}

// 3. Find Windows arm64 artifacts
const winArm64 = findFilePair('.msi', 'arm64');
if (winArm64 && winArm64.sigFile) {
  const filename = path.basename(winArm64.file);
  const signature = fs.readFileSync(winArm64.sigFile, 'utf8').trim();
  const url = `https://github.com/LookAtWhatAiCanDo/Codeoba/releases/download/${releaseTag}/${filename}`;
  
  mergedManifest.platforms['windows-aarch64'] = { signature, url };
  console.log(`✅ Added Windows arm64 (windows-aarch64) update target: ${filename}`);
} else {
  console.log('⚠️ No Windows arm64 (.msi) and signature (.msi.sig) pair found.');
}

// 4. Find Linux AppImage artifacts (Future proofing)
const linuxX64 = findFilePair('.AppImage', 'x86_64') || findFilePair('.AppImage', 'amd64');
if (linuxX64 && linuxX64.sigFile) {
  const filename = path.basename(linuxX64.file);
  const signature = fs.readFileSync(linuxX64.sigFile, 'utf8').trim();
  const url = `https://github.com/LookAtWhatAiCanDo/Codeoba/releases/download/${releaseTag}/${filename}`;
  
  mergedManifest.platforms['linux-x86_64'] = { signature, url };
  console.log(`✅ Added Linux x64 (linux-x86_64) update target: ${filename}`);
}

const linuxArm64 = findFilePair('.AppImage', 'arm64') || findFilePair('.AppImage', 'aarch64');
if (linuxArm64 && linuxArm64.sigFile) {
  const filename = path.basename(linuxArm64.file);
  const signature = fs.readFileSync(linuxArm64.sigFile, 'utf8').trim();
  const url = `https://github.com/LookAtWhatAiCanDo/Codeoba/releases/download/${releaseTag}/${filename}`;
  
  mergedManifest.platforms['linux-aarch64'] = { signature, url };
  console.log(`✅ Added Linux arm64 (linux-aarch64) update target: ${filename}`);
}

// Check if we registered any update targets
if (Object.keys(mergedManifest.platforms).length === 0) {
  console.warn('Warning: No updater targets were generated.');
} else {
  const outputPath = path.join(artifactsDir, 'latest.json');
  fs.writeFileSync(outputPath, JSON.stringify(mergedManifest, null, 2) + '\n');
  console.log(`\n✅ Generated latest.json successfully at: ${outputPath}`);
  console.log(JSON.stringify(mergedManifest, null, 2));
}
