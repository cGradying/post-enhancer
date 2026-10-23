#!/usr/bin/env bash
# Deploy Post Prep to AWS App Runner with an S3 bucket for batch history.
# Needs: aws CLI v2 (logged in), Docker running. Safe to re-run: it reuses what exists and redeploys.
set -euo pipefail

REGION="${AWS_REGION:-ap-southeast-1}"
APP="${APP:-post-prep}"
ACCOUNT="$(aws sts get-caller-identity --query Account --output text)"
BUCKET="${S3_BUCKET:-$APP-batches-$ACCOUNT}"
REPO="$ACCOUNT.dkr.ecr.$REGION.amazonaws.com/$APP"

echo "Account $ACCOUNT, region $REGION, bucket $BUCKET"

# 1. Private bucket
if ! aws s3api head-bucket --bucket "$BUCKET" 2>/dev/null; then
  if [ "$REGION" = us-east-1 ]; then
    aws s3api create-bucket --bucket "$BUCKET" --region "$REGION" >/dev/null
  else
    aws s3api create-bucket --bucket "$BUCKET" --region "$REGION" --create-bucket-configuration LocationConstraint="$REGION" >/dev/null
  fi
  aws s3api put-public-access-block --bucket "$BUCKET" --public-access-block-configuration \
    BlockPublicAcls=true,IgnorePublicAcls=true,BlockPublicPolicy=true,RestrictPublicBuckets=true
fi

# 2. Image
aws ecr describe-repositories --repository-names "$APP" --region "$REGION" >/dev/null 2>&1 \
  || aws ecr create-repository --repository-name "$APP" --region "$REGION" >/dev/null
aws ecr get-login-password --region "$REGION" | docker login --username AWS --password-stdin "${REPO%/*}"
docker build --platform linux/amd64 -t "$REPO:latest" .
docker push "$REPO:latest"

# 3. Roles: one lets App Runner pull from ECR, one lets the app read/write the bucket
role() { # name service
  aws iam get-role --role-name "$1" >/dev/null 2>&1 || aws iam create-role --role-name "$1" \
    --assume-role-policy-document "{\"Version\":\"2012-10-17\",\"Statement\":[{\"Effect\":\"Allow\",\"Principal\":{\"Service\":\"$2\"},\"Action\":\"sts:AssumeRole\"}]}" >/dev/null
}
role "$APP-ecr-access" build.apprunner.amazonaws.com
aws iam attach-role-policy --role-name "$APP-ecr-access" \
  --policy-arn arn:aws:iam::aws:policy/service-role/AWSAppRunnerServicePolicyForECRAccess
role "$APP-instance" tasks.apprunner.amazonaws.com
aws iam put-role-policy --role-name "$APP-instance" --policy-name s3-batches --policy-document "{
  \"Version\":\"2012-10-17\",\"Statement\":[
    {\"Effect\":\"Allow\",\"Action\":[\"s3:GetObject\",\"s3:PutObject\"],\"Resource\":\"arn:aws:s3:::$BUCKET/*\"},
    {\"Effect\":\"Allow\",\"Action\":\"s3:ListBucket\",\"Resource\":\"arn:aws:s3:::$BUCKET\"}]}"

# 4. Service
SERVICE_ARN="$(aws apprunner list-services --region "$REGION" \
  --query "ServiceSummaryList[?ServiceName=='$APP'].ServiceArn | [0]" --output text)"
if [ "$SERVICE_ARN" = None ] || [ -z "$SERVICE_ARN" ]; then
  read -rsp "Shared password for the app: " PASSWORD; echo
  sleep 10 # new IAM roles take a few seconds to become usable
  # ponytail: password sits in plain service env vars; move it to Secrets Manager if more than the team can see the console.
  SERVICE_ARN="$(aws apprunner create-service --region "$REGION" --service-name "$APP" \
    --source-configuration "{
      \"AuthenticationConfiguration\":{\"AccessRoleArn\":\"arn:aws:iam::$ACCOUNT:role/$APP-ecr-access\"},
      \"AutoDeploymentsEnabled\":false,
      \"ImageRepository\":{\"ImageIdentifier\":\"$REPO:latest\",\"ImageRepositoryType\":\"ECR\",
        \"ImageConfiguration\":{\"Port\":\"8080\",\"RuntimeEnvironmentVariables\":{
          \"APP_PASSWORD\":\"$PASSWORD\",\"S3_BUCKET\":\"$BUCKET\",\"AWS_REGION\":\"$REGION\"}}}}" \
    --instance-configuration "{\"Cpu\":\"1 vCPU\",\"Memory\":\"2 GB\",\"InstanceRoleArn\":\"arn:aws:iam::$ACCOUNT:role/$APP-instance\"}" \
    --health-check-configuration Protocol=TCP \
    --auto-scaling-configuration-arn "$(aws apprunner create-auto-scaling-configuration --region "$REGION" \
        --auto-scaling-configuration-name "$APP-single" --min-size 1 --max-size 1 \
        --query AutoScalingConfiguration.AutoScalingConfigurationArn --output text)" \
    --query Service.ServiceArn --output text)"
else
  aws apprunner start-deployment --region "$REGION" --service-arn "$SERVICE_ARN" >/dev/null
fi

echo "Deploying. URL:"
aws apprunner describe-service --region "$REGION" --service-arn "$SERVICE_ARN" --query Service.ServiceUrl --output text \
  | sed 's|^|https://|'
