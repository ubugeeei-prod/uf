locals {
  docs_hostname     = "docs.${var.zone_name}"
  setup_hostname    = "setup.${var.zone_name}"
  releases_hostname = "releases.${var.zone_name}"
  cache_hostname    = "cache.${var.zone_name}"

  docs_worker_name  = "uf-docs"
  root_worker_name  = "uf-root"
  setup_worker_name = "uf-setup"
}

resource "cloudflare_r2_bucket" "releases" {
  account_id    = var.account_id
  name          = var.release_bucket_name
  location      = var.r2_location
  storage_class = "Standard"
}

resource "cloudflare_r2_custom_domain" "releases" {
  account_id  = var.account_id
  bucket_name = cloudflare_r2_bucket.releases.name
  domain      = local.releases_hostname
  enabled     = true
  zone_id     = var.zone_id
}

resource "cloudflare_r2_bucket_cors" "releases" {
  account_id  = var.account_id
  bucket_name = cloudflare_r2_bucket.releases.name

  rules = [{
    id = "public-release-read"
    allowed = {
      methods = ["GET"]
      origins = [
        "https://${local.docs_hostname}",
        "https://${local.setup_hostname}",
      ]
      headers = ["*"]
    }
    expose_headers  = ["Content-Encoding", "Content-Length", "ETag"]
    max_age_seconds = 3600
  }]
}

resource "cloudflare_r2_bucket" "nix_cache" {
  count         = var.enable_nix_cache_bucket ? 1 : 0
  account_id    = var.account_id
  name          = var.nix_cache_bucket_name
  location      = var.r2_location
  storage_class = "Standard"
}

resource "cloudflare_r2_custom_domain" "nix_cache" {
  count       = var.enable_nix_cache_bucket ? 1 : 0
  account_id  = var.account_id
  bucket_name = cloudflare_r2_bucket.nix_cache[0].name
  domain      = local.cache_hostname
  enabled     = true
  zone_id     = var.zone_id
}

resource "cloudflare_workers_script" "setup" {
  account_id         = var.account_id
  script_name        = local.setup_worker_name
  compatibility_date = var.compatibility_date
  content_file       = "${path.module}/workers/setup.js"
  content_sha256     = filesha256("${path.module}/workers/setup.js")
  main_module        = "setup.js"

  assets = {
    directory        = "${path.module}/setup-assets"
    binding          = "ASSETS"
    run_worker_first = true
  }
}

resource "cloudflare_workers_custom_domain" "setup" {
  account_id = var.account_id
  hostname   = local.setup_hostname
  service    = cloudflare_workers_script.setup.script_name
  zone_name  = var.zone_name
}

resource "cloudflare_workers_script" "docs" {
  account_id         = var.account_id
  script_name        = local.docs_worker_name
  compatibility_date = var.compatibility_date
  content_file       = "${path.module}/workers/docs.js"
  content_sha256     = filesha256("${path.module}/workers/docs.js")
  main_module        = "docs.js"

  assets = {
    directory          = "${path.module}/../../docs/dist/docs"
    binding            = "ASSETS"
    not_found_handling = "404-page"
    html_handling      = "auto-trailing-slash"
    run_worker_first   = ["/setup", "/setup/", "/api/*"]
  }
}

resource "cloudflare_workers_custom_domain" "docs" {
  account_id = var.account_id
  hostname   = local.docs_hostname
  service    = cloudflare_workers_script.docs.script_name
  zone_name  = var.zone_name
}

resource "cloudflare_workers_script" "root" {
  account_id         = var.account_id
  script_name        = local.root_worker_name
  compatibility_date = var.compatibility_date
  content_file       = "${path.module}/workers/root.js"
  content_sha256     = filesha256("${path.module}/workers/root.js")
  main_module        = "root.js"
}

resource "cloudflare_workers_custom_domain" "apex" {
  account_id = var.account_id
  hostname   = var.zone_name
  service    = cloudflare_workers_script.root.script_name
  zone_name  = var.zone_name
}

resource "cloudflare_workers_custom_domain" "www" {
  account_id = var.account_id
  hostname   = "www.${var.zone_name}"
  service    = cloudflare_workers_script.root.script_name
  zone_name  = var.zone_name
}
