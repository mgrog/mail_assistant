/*
  Warnings:

  - You are about to drop the column `last_sync` on the `user_account_access` table. All the data in the column will be lost.

*/
-- AlterTable
ALTER TABLE "user" ADD COLUMN     "last_sync" TIMESTAMPTZ(6);

-- AlterTable
ALTER TABLE "user_account_access" DROP COLUMN "last_sync";
