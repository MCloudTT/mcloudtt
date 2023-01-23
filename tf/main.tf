terraform {
  required_providers {
    google = {
      source  = "hashicorp/google"
      version = "3.63.0"
    }
  }

  backend "local" {
    path = "../../tf/terraform.tfstate"
  }
}

provider "google" {
  project = "azubi-knowhow-building"
  region  = "us-central1"
}

resource "google_container_cluster" "dev" {
  name     = "mcloudtt-dev-cluster"
  location = "us-central1"

  network    = "mcloudtt-vpc"
  subnetwork = "mcloudtt-vpc"

  # Enabling Autopilot for this cluster
  enable_autopilot = true
}