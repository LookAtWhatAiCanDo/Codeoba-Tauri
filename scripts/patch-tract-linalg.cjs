const fs = require('fs');
const path = require('path');
const os = require('os');

// 1. Locate the .cargo registry directory
const homeDir = os.homedir();
const registrySrcDir = path.join(homeDir, '.cargo', 'registry', 'src');

if (!fs.existsSync(registrySrcDir)) {
  console.error(`Cargo registry src directory not found at: ${registrySrcDir}`);
  process.exit(1);
}

// 2. Find the tract-linalg directory
let tractLinalgDirs = [];
function findTractLinalg(dir) {
  try {
    const files = fs.readdirSync(dir);
    for (const file of files) {
      const fullPath = path.join(dir, file);
      const stat = fs.statSync(fullPath);
      if (stat.isDirectory()) {
        if (file.startsWith('tract-linalg-')) {
          tractLinalgDirs.push(fullPath);
        } else if (dir === registrySrcDir) {
          // Recurse into index directories (e.g. index.crates.io-...)
          findTractLinalg(fullPath);
        }
      }
    }
  } catch (e) {
    console.error(`Error reading directory ${dir}:`, e);
  }
}

findTractLinalg(registrySrcDir);

if (tractLinalgDirs.length === 0) {
  console.error('No tract-linalg directories found in Cargo registry cache.');
  process.exit(1);
}

console.log(`Found tract-linalg directories:`, tractLinalgDirs);

// 3. Patch build.rs in each found directory
for (const dir of tractLinalgDirs) {
  const buildRsPath = path.join(dir, 'build.rs');
  if (fs.existsSync(buildRsPath)) {
    console.log(`Patching ${buildRsPath}...`);
    let content = fs.readFileSync(buildRsPath, 'utf8');
    
    // Modify use_masm() to return false for aarch64 target architecture
    const originalUseMasm = 'fn use_masm() -> bool {';
    const patchedUseMasm = 'fn use_masm() -> bool {\n    if env::var("CARGO_CFG_TARGET_ARCH") == Ok("aarch64".to_string()) { return false; }';
    
    if (content.includes(patchedUseMasm)) {
      console.log('Already patched.');
      continue;
    }
    
    if (content.includes(originalUseMasm)) {
      content = content.replace(originalUseMasm, patchedUseMasm);
      fs.writeFileSync(buildRsPath, content, 'utf8');
      console.log('Successfully patched use_masm in build.rs!');
    } else {
      console.error('Could not find use_masm function to patch!');
    }
  }
}
