/*
  Warnings:

  - You are about to drop the column `after_days_old` on the `custom_email_rule` table. All the data in the column will be lost.
  - You are about to drop the column `associated_email_client_category` on the `custom_email_rule` table. All the data in the column will be lost.
  - You are about to drop the column `cleanup_action` on the `custom_email_rule` table. All the data in the column will be lost.
  - You are about to drop the column `after_days_old` on the `default_email_rule_override` table. All the data in the column will be lost.
  - You are about to drop the column `associated_email_client_category` on the `default_email_rule_override` table. All the data in the column will be lost.
  - You are about to drop the column `cleanup_action` on the `default_email_rule_override` table. All the data in the column will be lost.

*/
-- AlterTable
ALTER TABLE "custom_email_rule" DROP COLUMN "after_days_old",
DROP COLUMN "associated_email_client_category",
DROP COLUMN "cleanup_action";

-- AlterTable
ALTER TABLE "default_email_rule_override" DROP COLUMN "after_days_old",
DROP COLUMN "associated_email_client_category",
DROP COLUMN "cleanup_action";

-- CreateTable
CREATE TABLE "auto_cleanup_setting" (
    "id" SERIAL NOT NULL,
    "user_id" INTEGER NOT NULL,
    "category" VARCHAR NOT NULL,
    "is_disabled" BOOLEAN NOT NULL DEFAULT false,
    "after_days_old" INTEGER NOT NULL DEFAULT 7,
    "cleanup_action" "cleanup_action" NOT NULL DEFAULT 'NOTHING',
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updated_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "auto_cleanup_setting_pkey" PRIMARY KEY ("id")
);

-- CreateIndex
CREATE INDEX "auto_cleanup_setting_user_id_idx" ON "auto_cleanup_setting"("user_id");

-- CreateIndex
CREATE UNIQUE INDEX "auto_cleanup_setting_category_user_id_key" ON "auto_cleanup_setting"("category", "user_id");

-- AddForeignKey
ALTER TABLE "auto_cleanup_setting" ADD CONSTRAINT "auto_cleanup_setting_user_id_fkey" FOREIGN KEY ("user_id") REFERENCES "user"("id") ON DELETE CASCADE ON UPDATE CASCADE;
