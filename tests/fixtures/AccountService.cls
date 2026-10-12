public with sharing class AccountService {
    @AuraEnabled
    public static List<Account> getActiveAccounts() {
        return [SELECT Id, Name, Rating__c FROM Account WHERE Active__c = 'Yes'];
    }

    public void updateAccountRating(Id accountId, String rating) {
        // Business logic for updating rating
    }
}
