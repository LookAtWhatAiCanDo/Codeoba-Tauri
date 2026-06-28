const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');

const args = process.argv.slice(2);
const commitFlagIndex = args.indexOf('--commit');
const commitShortIndex = args.indexOf('-c');
const shouldCommit = commitFlagIndex !== -1 || commitShortIndex !== -1;

// Filter out flag arguments to find the version string
const positionalArgs = args.filter(arg => arg !== '--commit' && arg !== '-c');
const version = positionalArgs[0];

if (!version) {
  console.error('Usage: node scripts/bump-version.cjs <new-version> [--commit | -c]');
  process.exit(1);
}

// Basic SemVer validation regex
const semverRegex = /^\d+\.\d+\.\d+(-[a-zA-Z0-9.]+)?$/;
if (!semverRegex.test(version)) {
  console.error(`Error: "${version}" is not a valid Semantic Version (must match X.Y.Z[-pre]).`);
  process.exit(1);
}

let hasStashed = false;
try {
  if (shouldCommit) {
    try {
      const status = execSync('git status --porcelain', { encoding: 'utf8' }).trim();
      if (status) {
        console.log('Stashing local changes to ensure a clean version bump commit...');
        execSync('git stash', { stdio: 'inherit' });
        hasStashed = true;
      }
    } catch (err) {
      console.warn('Warning: Failed to check or save git stash:', err.message);
    }
  }

  console.log(`Bumping project versions to: ${version}`);

  // 1. Update package.json
  const pkgPath = path.join(__dirname, '../package.json');
  if (fs.existsSync(pkgPath)) {
    const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
    pkg.version = version;
    fs.writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + '\n');
    console.log(`Updated package.json version to ${version}`);
  }

  // 2. Update package-lock.json
  const lockPath = path.join(__dirname, '../package-lock.json');
  if (fs.existsSync(lockPath)) {
    const lock = JSON.parse(fs.readFileSync(lockPath, 'utf8'));
    if (lock.version) lock.version = version;
    if (lock.packages && lock.packages['']) {
      lock.packages[''].version = version;
    }
    fs.writeFileSync(lockPath, JSON.stringify(lock, null, 2) + '\n');
    console.log(`Updated package-lock.json version to ${version}`);
  }

  // 3. Update tauri.conf.json
  const confPath = path.join(__dirname, '../src-tauri/tauri.conf.json');
  if (fs.existsSync(confPath)) {
    const conf = JSON.parse(fs.readFileSync(confPath, 'utf8'));
    conf.version = version;
    fs.writeFileSync(confPath, JSON.stringify(conf, null, 2) + '\n');
    console.log(`Updated tauri.conf.json version to ${version}`);
  }

  // 4. Update Cargo.toml (Section-aware line parser)
  const cargoPath = path.join(__dirname, '../src-tauri/Cargo.toml');
  if (fs.existsSync(cargoPath)) {
    const content = fs.readFileSync(cargoPath, 'utf8');
    const lines = content.split(/\r?\n/);
    let inPackageSection = false;
    let updated = false;

    for (let i = 0; i < lines.length; i++) {
      const trimmed = lines[i].trim();
      if (trimmed.startsWith('[package]')) {
        inPackageSection = true;
      } else if (trimmed.startsWith('[') && trimmed.endsWith(']')) {
        inPackageSection = false;
      }

      if (inPackageSection && trimmed.startsWith('version =')) {
        const match = lines[i].match(/^(\s*version\s*=\s*['"])[^'"]*(['"].*)$/);
        if (match) {
          lines[i] = `${match[1]}${version}${match[2]}`;
        } else {
          lines[i] = `version = "${version}"`;
        }
        updated = true;
        break;
      }
    }

    if (updated) {
      fs.writeFileSync(cargoPath, lines.join('\n'));
      console.log(`Updated Cargo.toml version to ${version}`);

      try {
        console.log('Syncing Cargo.lock...');
        execSync('cargo update -p codeoba', {
          cwd: path.dirname(cargoPath),
          stdio: 'inherit'
        });
        console.log('Updated Cargo.lock successfully');
      } catch (err) {
        console.warn(`Warning: Could not update Cargo.lock automatically: ${err.message}`);
      }
    } else {
      console.warn('Could not find package version in Cargo.toml');
    }
  }

  // 5. Optional git commit
  if (shouldCommit) {
    try {
      console.log('Staging modified version files...');
      execSync('git add package.json package-lock.json src-tauri/tauri.conf.json src-tauri/Cargo.toml src-tauri/Cargo.lock', { stdio: 'inherit' });
      
      const commitMessage = `chore(release): bump version to v${version}`;
      console.log(`Committing: "${commitMessage}"`);
      execSync(`git commit -m "${commitMessage}"`, { stdio: 'inherit' });
      console.log('✅ Version bump committed successfully!');
    } catch (err) {
      console.error('Error committing changes:', err.message);
      process.exit(1);
    }
  }
} finally {
  if (hasStashed) {
    try {
      console.log('Restoring stashed local changes...');
      execSync('git stash pop', { stdio: 'inherit' });
    } catch (err) {
      console.error('Warning: Failed to restore stashed changes automatically. Run "git stash pop" manually.', err.message);
    }
  }
}
