# Intro

A tiny [controller](https://kubernetes.io/docs/concepts/architecture/controller/) that manages CIDR blocks in AWS, on Security Groups, EKS access list etc..

The name is also a pun on [our company name](https://controlant.com/).

# Usage

## Concept

- Run the controller with desired CIDR mappings and tag names
- Apply tags to AWS resources, Security Groups and EKS clusters are currently supported
- The controller will manage CIDR rules

## Detail

- Use `--help` to see documentation for cli arguments
- `--ingress-key` can be used to specify different tag key
- Configure tags for AWS resources:
  - `[key]/sources` are used to configure what CIDR blocks would be applied to a certain resource
  - `[key]/ports` are used to configure port and protocols
  - Use colon (`:`) to separate multiple values
    - this is due to comma (`,`) is not allowed for tag values in AWS

## Example

- Run `ctrl-cidr --cidr office=1.2.3.4/32 --cidr vpn 5.6.7.8/32 --ingress-key ingress`
- Add tag `ingress/sources` with value `office:vpn` to a Security Group
- Add tag `ingress/ports` with value `443:4567/udp` to the same Security Group
- The controller will maintain following rules for the Security Group:
  - ingress from `1.2.3.4/32` on port `443` with protocol `tcp` (default)
  - ingress from `1.2.3.4/32` on port `4567` with protocol `udp`
  - ingress from `5.6.7.8/32` on port `443` with protocol `tcp`
  - ingress from `5.6.7.8/32` on port `4567` with protocol `udp`

To allow all protocols, set the `ingress/ports` tag to `-1/-1`, this is a special case and might change in the future.

## Assume Role and AWS Account Discovery

The controller is designed with managing multiple AWS accounts in mind:

- Without any related arguments, the controller will use the [default credential provider chain](https://docs.rs/aws-config/latest/aws_config/default_provider/credentials/struct.DefaultCredentialsChain.html), and run in the configured AWS account
- Using the `--assume-role` arguments (repeatable), the controller will try to assume all the specified roles and perform changes in those accounts, these roles need to have sufficient permission, which is listed in the next section (and obviously need to allow the controller to assume such roles)
- Alternatively, the controller can also be instructed to discover all accounts in an organization, and try to assume role based on the same path and name. The `--sub-role` must include the path and name of the role (e.g. `/controllers/cidr-controller`), this option is also mutually excluse with `--assume-role`
  - By default, the controller uses the current credential to discover accounts, which requires it to be run within the root account. This is not always desirable, so the `--root-role` allows the controller to assume a role in the root account to perform discovery, which enables it to be run in other places. The "root role" needs to allow `organizations:ListAccounts` in its IAM policy

### "sub account" IAM policy (in Terraform)

Note: change the `INGRESS_KEY` below:

```hcl
data "aws_iam_policy_document" "perms" {
  statement {
    actions = [
      "ec2:AuthorizeSecurityGroupIngress",

      "eks:UpdateClusterConfig",
    ]
    effect    = "Allow"
    resources = ["*"]

    condition {
      test     = "Null"
      variable = "aws:ResourceTag/INGRESS_KEY/sources"
      values   = ["false"]
    }
  }

  statement {
    actions = [
      "ec2:DescribeSecurityGroups",
      "ec2:DescribeSecurityGroupRules",

      "eks:ListClusters",
      "eks:DescribeCluster",
    ]
    effect    = "Allow"
    resources = ["*"]
  }
}
```

## CIDR Deprecation

To deprecate CIDRs:

- Run `ctrl-cidr --cidr office=4.3.2.1/32 --deprecate 1.2.3.4/32 --deprecate 5.6.7.8/32 --ingress_key ingress`
- For Security Groups, the controller puts a marker as description, so that it can detect all rules it manages, and will automatically remove old / unused ones
- For EKS clusters, because there's no place to put a marker for each ingress CIDR, we use the `--deprecate` argument to instruct the controller to remove old rules
- If the controller is not used for EKS cluster, the `--deprecate` arguments are unnecessary
  - in the future we have plan to use a "statefile" to track these
- Due to permission concerns, to opt-out of all CIDR mappings, a resource should not directly remove the tags, but only set the value to empty (e.g. `ingress/sources=`) after an update cycle, the tags can be deleted if so desired

# Roadmap

- TODO: add metrics
- TODO: use distributed tracing
- TODO: more tests
- MAYBE: implement egress control
- MAYBE: implement a "statefile" for automated cleanup (remove `--deprecate`)
- MAYBE: support ipv6
- MAYBE: manage VPC ACLs

# Debug

- `RUST_LOG=trace` will enable all logs including those from AWS SDK
- `RUST_LOG=warn,ctrl_cidr=debug` will enable debug+ logs from the controller (these include all read api calls and no action decisions) as well as warn+ logs from other crates
- info logs include all update decisions and write api call results
