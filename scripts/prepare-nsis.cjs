const fs = require('fs');
const path = require('path');
const https = require('https');

const destDir = path.join(__dirname, '..', 'src-tauri', 'nsis');
const destFile = path.join(destDir, 'installer.nsi');
const templateUrl = 'https://raw.githubusercontent.com/tauri-apps/tauri/dev/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi';

function ensureDirectoryExistence(filePath) {
  const dirname = path.dirname(filePath);
  if (fs.existsSync(dirname)) {
    return true;
  }
  ensureDirectoryExistence(dirname);
  fs.mkdirSync(dirname);
}

if (fs.existsSync(destFile)) {
  console.log('Tauri custom NSIS template already exists, skipping fetch.');
  process.exit(0);
}

ensureDirectoryExistence(destFile);

console.log(`Fetching default NSIS template from ${templateUrl}...`);

https.get(templateUrl, (res) => {
  if (res.statusCode !== 200) {
    console.error(`Failed to fetch default template: status code ${res.statusCode}`);
    process.exit(1);
  }

  let data = '';
  res.on('data', (chunk) => {
    data += chunk;
  });

  res.on('end', () => {
    try {
      console.log('NSIS template downloaded successfully. Applying path overrides...');

      // Replace MULTIUSER_INSTALLMODE_INSTDIR
      const oldMultiuser = '!define MULTIUSER_INSTALLMODE_INSTDIR "${PRODUCTNAME}"';
      const newMultiuser = '!define MULTIUSER_INSTALLMODE_INSTDIR "${MANUFACTURER}\\${PRODUCTNAME}"';
      if (!data.includes(oldMultiuser)) {
        throw new Error('Could not find MULTIUSER_INSTALLMODE_INSTDIR definition in the template.');
      }
      data = data.replace(oldMultiuser, newMultiuser);

      // Replace $INSTDIR assignments under perMachine
      const oldPf64 = 'StrCpy $INSTDIR "$PROGRAMFILES64\\${PRODUCTNAME}"';
      const newPf64 = 'StrCpy $INSTDIR "$PROGRAMFILES64\\${MANUFACTURER}\\${PRODUCTNAME}"';
      if (!data.includes(oldPf64)) {
        throw new Error('Could not find PROGRAMFILES64 assignment in the template.');
      }
      data = data.replace(new RegExp(escapeRegExp(oldPf64), 'g'), newPf64);

      const oldPf32 = 'StrCpy $INSTDIR "$PROGRAMFILES\\${PRODUCTNAME}"';
      const newPf32 = 'StrCpy $INSTDIR "$PROGRAMFILES\\${MANUFACTURER}\\${PRODUCTNAME}"';
      if (!data.includes(oldPf32)) {
        throw new Error('Could not find PROGRAMFILES assignment in the template.');
      }
      data = data.replace(new RegExp(escapeRegExp(oldPf32), 'g'), newPf32);

      // Replace $INSTDIR assignments under currentUser
      const oldLa = 'StrCpy $INSTDIR "$LOCALAPPDATA\\${PRODUCTNAME}"';
      const newLa = 'StrCpy $INSTDIR "$LOCALAPPDATA\\${MANUFACTURER}\\${PRODUCTNAME}"';
      if (!data.includes(oldLa)) {
        throw new Error('Could not find LOCALAPPDATA assignment in the template.');
      }
      data = data.replace(new RegExp(escapeRegExp(oldLa), 'g'), newLa);

      fs.writeFileSync(destFile, data, 'utf8');
      console.log(`Successfully generated custom NSIS template at: ${destFile}`);
    } catch (e) {
      console.error('Error processing NSIS template:', e.message);
      process.exit(1);
    }
  });
}).on('error', (err) => {
  console.error('Network error fetching NSIS template:', err.message);
  process.exit(1);
});

function escapeRegExp(string) {
  return string.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}
