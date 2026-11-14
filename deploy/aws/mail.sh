# shellcheck shell=bash disable=SC2154  # settings, state paths and helpers come from env.sh
# Agent email (E4), sourced by env.sh. The mail stack is permanent, like the
# foundation: it keeps SES's queue and stored mail while environments come and
# go. See runtime/email-channel.md.
#
#   mail [DOMAIN]   create or update the mail stack (default mail.$AYATI_DOMAIN),
#                   make its receipt rules the region's active ones and show
#                   whether SES has verified the domain

mail_stack=ayati-mail

# The controller's AYATI_EMAIL_FILE, or nothing when the mail stack doesn't exist.
mail_settings() {
    deploy_aws cloudformation describe-stacks --stack-name "$mail_stack" \
        --query "Stacks[0].Outputs[?OutputKey=='Settings'].OutputValue" --output text 2>/dev/null || true
}

mail() {
    local domain=${1:-mail.$AYATI_DOMAIN}
    layer=permanent deploy_stack "$mail_stack" mail.yaml MailDomain="$domain"
    # Only one receipt rule set is active per region; CloudFormation can't choose it.
    deploy_aws ses set-active-receipt-rule-set --rule-set-name ayati-mail
    say "Mail domain $domain"
    deploy_aws sesv2 get-email-identity --email-identity "$domain" \
        --query '{verified:VerifiedForSendingStatus,dkim:DkimAttributes.Status,mail_from:MailFromAttributes.MailFromDomainStatus}' \
        --output table
    deploy_aws sesv2 get-account --query '{production_access:ProductionAccessEnabled,sending:SendingEnabled}' --output table
    echo "Run env.sh deploy (or ship) so the controllers read the mail queue."
}
