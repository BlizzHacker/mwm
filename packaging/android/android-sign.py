#!/usr/bin/env python3
"""Inject release signing into the Tauri-generated Android gradle project.

Run after `cargo tauri android init`. Reads gen/android/keystore.properties and
adds a signingConfig to app/build.gradle.kts so the APK/AAB are signed.
Idempotent: skips if a signingConfig named 'release' already exists.
"""
import pathlib, sys

android_dir = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "app/src-tauri/gen/android")
bg = android_dir / "app" / "build.gradle.kts"
if not bg.exists():
    sys.exit(f"missing {bg}")
s = bg.read_text()

if "signingConfigs" in s:
    print("signing already present, skipping")
    sys.exit(0)

keystore_props_loader = '''val keystorePropertiesFile = rootProject.file("keystore.properties")
val keystoreProperties = Properties()
if (keystorePropertiesFile.exists()) {
    keystorePropertiesFile.inputStream().use { keystoreProperties.load(it) }
}

'''

signing_config = '''    signingConfigs {
        if (keystorePropertiesFile.exists()) {
            create("release") {
                keyAlias = keystoreProperties["keyAlias"] as String
                keyPassword = keystoreProperties["password"] as String
                storeFile = file(keystoreProperties["storeFile"] as String)
                storePassword = keystoreProperties["storePassword"] as String
                val st = keystoreProperties["storeType"] as String?
                if (!st.isNullOrBlank()) { storeType = st }
            }
        }
    }
'''

signing_ref = '''            if (keystorePropertiesFile.exists()) {
                signingConfig = signingConfigs.getByName("release")
            }
'''

# 1) Insert the properties loader before the `android {` block
assert "android {" in s, "no android block"
s = s.replace("android {\n", keystore_props_loader + "android {\n", 1)

# 2) Insert signingConfigs right before buildTypes
assert "    buildTypes {" in s, "no buildTypes"
s = s.replace("    buildTypes {", signing_config + "    buildTypes {", 1)

# 3) Reference the signing config inside getByName("release")
assert 'getByName("release") {' in s, "no release buildType"
s = s.replace('getByName("release") {\n', 'getByName("release") {\n' + signing_ref, 1)

bg.write_text(s)
print("signing injected into", bg)
