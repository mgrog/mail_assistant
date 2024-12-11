-- AlterTable
ALTER TABLE "custom_email_rule" ADD COLUMN     "associated_email_client_category" "AssociatedEmailClientCategory";

-- AlterTable
ALTER TABLE "default_email_rule_override" ADD COLUMN     "associated_email_client_category" "AssociatedEmailClientCategory";
