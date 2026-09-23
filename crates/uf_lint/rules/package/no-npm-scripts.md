Install-time lifecycle scripts (`preinstall`, `install`, `postinstall`, `prepare`) run arbitrary code on every machine that installs the package, which is how most npm supply-chain attacks spread. uf refuses them unless you allow them explicitly. Named scripts that only run when someone asks for them are fine.

## Bad

```json
{
  "name": "shop",
  "scripts": { "postinstall": "node scripts/setup.js" }
}
```

```diagnostics
package.json:3:3 install-time lifecycle scripts (postinstall) are disabled; move the automation to uf tasks or explicitly allow lifecycle scripts
```

## Good

```json
{
  "name": "shop",
  "scripts": { "ios": "expo start --ios", "start": "expo start" }
}
```
