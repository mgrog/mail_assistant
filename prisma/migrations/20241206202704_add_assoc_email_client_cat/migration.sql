-- CreateEnum
CREATE TYPE "AssociatedEmailClientCategory" AS ENUM ('CATEGORY_PERSONAL', 'CATEGORY_SOCIAL', 'CATEGORY_PROMOTIONS', 'CATEGORY_UPDATES');

-- AlterTable
ALTER TABLE "custom_email_rule" ADD COLUMN     "associated_email_client_category" "AssociatedEmailClientCategory";

-- AlterTable
ALTER TABLE "default_email_rule_override" ADD COLUMN     "associated_email_client_category" "AssociatedEmailClientCategory";
