variable "account_id" {
  description = "Cloudflare account ID that owns uniflowed.dev and the R2 buckets."
  type        = string
}

variable "zone_name" {
  description = "Cloudflare zone name."
  type        = string
  default     = "uniflowed.dev"
}

variable "zone_id" {
  description = "Cloudflare zone ID for uniflowed.dev."
  type        = string
}

variable "compatibility_date" {
  description = "Workers compatibility date."
  type        = string
  default     = "2026-09-02"
}

variable "release_bucket_name" {
  description = "R2 bucket used for public uf release artifacts."
  type        = string
  default     = "uf-releases"
}

variable "nix_cache_bucket_name" {
  description = "R2 bucket reserved for the public Nix binary cache."
  type        = string
  default     = "uf-nix-cache"
}

variable "r2_location" {
  description = "R2 location hint for newly created buckets."
  type        = string
  default     = "apac"
}

variable "enable_nix_cache_bucket" {
  description = "Create cache.uniflowed.dev and the backing R2 bucket for future Nix binary cache objects."
  type        = bool
  default     = true
}
