output "docs_url" {
  value = "https://${local.docs_hostname}"
}

output "setup_url" {
  value = "https://${local.setup_hostname}"
}

output "root_url" {
  value = "https://${var.zone_name}"
}

output "releases_url" {
  value = "https://${local.releases_hostname}/uf"
}

output "nix_cache_url" {
  value = var.enable_nix_cache_bucket ? "https://${local.cache_hostname}" : null
}

output "release_bucket_name" {
  value = cloudflare_r2_bucket.releases.name
}
