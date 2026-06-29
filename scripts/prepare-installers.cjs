const fs = require('fs');
const path = require('path');
const https = require('https');

const nsisDir = path.join(__dirname, '..', 'src-tauri', 'nsis');
const nsisFile = path.join(nsisDir, 'installer.nsi');
const wixFile = path.join(__dirname, '..', 'src-tauri', 'main.wxs');

const nsisUrl = 'https://raw.githubusercontent.com/tauri-apps/tauri/dev/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi';
const wixUrl = 'https://raw.githubusercontent.com/tauri-apps/tauri/next/tooling/bundler/src/bundle/windows/templates/main.wxs';

function ensureDirectoryExistence(filePath) {
  const dirname = path.dirname(filePath);
  if (fs.existsSync(dirname)) {
    return true;
  }
  ensureDirectoryExistence(dirname);
  fs.mkdirSync(dirname);
}

function escapeRegExp(string) {
  return string.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function downloadFile(url, dest, patchFn, callback) {
  if (fs.existsSync(dest)) {
    console.log(`${path.basename(dest)} already exists, skipping download.`);
    return callback(null);
  }
  
  ensureDirectoryExistence(dest);
  console.log(`Downloading ${path.basename(dest)} from ${url}...`);
  
  https.get(url, (res) => {
    if (res.statusCode !== 200) {
      return callback(new Error(`Failed to fetch ${path.basename(dest)}: status code ${res.statusCode}`));
    }
    
    let data = '';
    res.on('data', (chunk) => { data += chunk; });
    res.on('end', () => {
      try {
        const modified = patchFn(data);
        fs.writeFileSync(dest, modified, 'utf8');
        console.log(`Successfully generated ${path.basename(dest)} at ${dest}`);
        callback(null);
      } catch (err) {
        callback(err);
      }
    });
  }).on('error', (err) => {
    callback(err);
  });
}

// 1. Prepare NSIS
const patchNsis = (data) => {
  console.log('Applying NSIS path overrides...');
  const oldMultiuser = '!define MULTIUSER_INSTALLMODE_INSTDIR "${PRODUCTNAME}"';
  const newMultiuser = '!define MULTIUSER_INSTALLMODE_INSTDIR "${MANUFACTURER}\\${PRODUCTNAME}"';
  if (!data.includes(oldMultiuser)) {
    throw new Error('Could not find MULTIUSER_INSTALLMODE_INSTDIR definition in the NSIS template.');
  }
  data = data.replace(oldMultiuser, newMultiuser);

  const oldPf64 = 'StrCpy $INSTDIR "$PROGRAMFILES64\\${PRODUCTNAME}"';
  const newPf64 = 'StrCpy $INSTDIR "$PROGRAMFILES64\\${MANUFACTURER}\\${PRODUCTNAME}"';
  if (!data.includes(oldPf64)) {
    throw new Error('Could not find PROGRAMFILES64 assignment in the NSIS template.');
  }
  data = data.replace(new RegExp(escapeRegExp(oldPf64), 'g'), newPf64);

  const oldPf32 = 'StrCpy $INSTDIR "$PROGRAMFILES\\${PRODUCTNAME}"';
  const newPf32 = 'StrCpy $INSTDIR "$PROGRAMFILES\\${MANUFACTURER}\\${PRODUCTNAME}"';
  if (!data.includes(oldPf32)) {
    throw new Error('Could not find PROGRAMFILES assignment in the NSIS template.');
  }
  data = data.replace(new RegExp(escapeRegExp(oldPf32), 'g'), newPf32);

  const oldLa = 'StrCpy $INSTDIR "$LOCALAPPDATA\\${PRODUCTNAME}"';
  const newLa = 'StrCpy $INSTDIR "$LOCALAPPDATA\\${MANUFACTURER}\\${PRODUCTNAME}"';
  if (!data.includes(oldLa)) {
    throw new Error('Could not find LOCALAPPDATA assignment in the NSIS template.');
  }
  data = data.replace(new RegExp(escapeRegExp(oldLa), 'g'), newLa);

  return data;
};

// 2. Prepare WiX
const patchWix = (data) => {
  console.log('Applying WiX path overrides...');
  const oldFallback = '<Directory Id="INSTALLDIR" Name="{{product_name}}"/>';
  const newFallback = '<Directory Id="INSTALLDIR" Name="{{manufacturer}}\\{{product_name}}"/>';
  if (!data.includes(oldFallback)) {
    throw new Error('Could not find directory structure to patch in the WiX template.');
  }
  data = data.replace(oldFallback, newFallback);
  return data;
};

// Run downloads sequentially
downloadFile(nsisUrl, nsisFile, patchNsis, (err) => {
  if (err) {
    console.error('NSIS Error:', err.message);
    process.exit(1);
  }
  
  downloadFile(wixUrl, wixFile, patchWix, (err) => {
    if (err) {
      console.error('WiX Error:', err.message);
      process.exit(1);
    }
    console.log('All installer templates prepared successfully.');
    process.exit(0);
  });
});
