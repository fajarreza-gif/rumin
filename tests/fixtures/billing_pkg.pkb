-- Oracle PL/SQL Package Body fixture for testing ai-rumin
CREATE OR REPLACE PACKAGE billing_manager AS
    PROCEDURE process_invoice(inv_id NUMBER, amount NUMBER);
    FUNCTION calculate_tax(amount NUMBER) RETURN NUMBER;
END billing_manager;
/

CREATE OR REPLACE PACKAGE BODY billing_manager AS
    PROCEDURE process_invoice(inv_id NUMBER, amount NUMBER) IS
    BEGIN
        UPDATE invoices SET status = 'PAID', paid_amount = amount WHERE id = inv_id;
    END process_invoice;

    FUNCTION calculate_tax(amount NUMBER) RETURN NUMBER IS
    BEGIN
        RETURN amount * 0.0825;
    END calculate_tax;
END billing_manager;
/

CREATE OR REPLACE TRIGGER trg_invoice_audit
BEFORE UPDATE ON invoices
FOR EACH ROW
BEGIN
    INSERT INTO invoice_audit (invoice_id, old_status, changed_at)
    VALUES (:OLD.id, :OLD.status, SYSDATE);
END;
/

CREATE OR REPLACE TYPE invoice_item_t AS OBJECT (
    item_id NUMBER,
    description VARCHAR2(100),
    unit_price NUMBER
);
/

CREATE SEQUENCE invoice_seq START WITH 1000 INCREMENT BY 1;
