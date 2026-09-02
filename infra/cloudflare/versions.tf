terraform {
  required_version = ">= 1.8.0"

  required_providers {
    cloudflare = {
      source  = "cloudflare/cloudflare"
      version = ">= 5.11.0, < 6.0.0"
    }
  }
}

provider "cloudflare" {}
