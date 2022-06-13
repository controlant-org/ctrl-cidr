# Intro

A tiny [controller](https://kubernetes.io/docs/concepts/architecture/controller/) that manages CIDR blocks in AWS, on Security Groups, EKS access list etc..

The name is also a pun on [our company name](https://controlant.com/).

# Usage

## Concept

- Run the controller (this project) with desired CIDR mappings and tag names
- Apply tags to AWS resources like Security Groups and EKS clusters
- The controller will manage rules

## Detail

- Use `--help` to see documentation for cli arguments
- `ingress_key` ~~and `egress_key`~~ (currently not implemented) are configurable
- `[key]/sources` are used to configure what CIDR blocks would be applied to a certain resource (on ingress or egress)
  - Note EKS cluster only checks for ingress
- `[key]/ports` are used to configure port and protocols
- Use colon (`:`) to separate multiple values
  - this is due to comma (`,`) is not allowed for tag values in AWS

## Example

- Run `ctrl-cidr --cidr office=1.2.3.4/32 --cidr vpn 5.6.7.8/32 --ingress_key ingress`
- Add tag `ingress/sources` with value `office:vpn` to a Security Group
- Add tag `ingress/ports` with value `443/tcp:4567/-1` to the same Security Group
- The controller will maintain following rules for the Security Group:
  - ingress from `1.2.3.4/32` on port `443` with protocol `tcp`
  - ingress from `1.2.3.4/32` on port `4567` with protocol `-1` (all protocols)
  - ingress from `5.6.7.8/32` on port `443` with protocol `tcp`
  - ingress from `5.6.7.8/32` on port `4567` with protocol `-1` (all protocols)

# Roadmap

- TODO: allow specify protocol
- TODO: metrics
- TODO: use distributed tracing (probably helps with async as well)
- TODO: add tests
- MAYBE: implement egress control
- MAYBE: implement a "statefile" for better cleanup tracking
- MAYBE: support ipv6
- MAYBE: manage VPC ACLs
