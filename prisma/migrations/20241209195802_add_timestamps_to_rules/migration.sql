-- AlterTable
ALTER TABLE "custom_email_rule" ADD COLUMN     "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
ADD COLUMN     "updated_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP;

-- AlterTable
ALTER TABLE "default_email_rule_override" ADD COLUMN     "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
ADD COLUMN     "updated_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP;

-- CreateIndex
CREATE INDEX "custom_email_rule_user_id_updated_at_idx" ON "custom_email_rule"("user_id", "updated_at" DESC);

-- CreateIndex
CREATE INDEX "default_email_rule_override_user_id_updated_at_idx" ON "default_email_rule_override"("user_id", "updated_at" DESC);
