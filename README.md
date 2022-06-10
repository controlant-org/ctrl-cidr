# Intro

A tiny [controller](https://kubernetes.io/docs/concepts/architecture/controller/) that manages CIDR blocks in AWS, on Security Groups, EKS access list etc..

The name is also a pun on [our company name](https://controlant.com/).

# Usage

- Run the controller (this project) with desired CIDR mappings and tag names
- Apply tags to AWS resources like Security Groups and EKS clusters
  - Note, not all tags are supported on all resources
- The controller will manage rules

# Roadmap

- TODO: use assumerole to manage multiple accounts
- MAYBE: manage VPC ACLs
- TODO: keep a "statefile" for cleanup purpose
- TODO: allow specify protocol
- MAYBE: support ipv6
- TODO: metrics
- MAYBE: use distributed tracing (probably helps with async as well)
