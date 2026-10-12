trigger AccountTrigger on Account (before insert, after update) {
    if (Trigger.isBefore) {
        // Enforce account validations
    }
}
