use ai_rumin::graph::CodeGraph;
use ai_rumin::hippocampal::HippocampalEngine;
use ai_rumin::indexer::Indexer;
use ai_rumin::parsers;
use ai_rumin::sidecar::SidecarClient;
use ai_rumin::types::{ContextBudget, EntityType};
use std::fs;

// =============================================================================
// 1. VB.NET (.vb) COMPREHENSIVE PARSER TEST
// =============================================================================
#[test]
fn test_vb_net_comprehensive_parser() {
    let mut graph = CodeGraph::new();
    let vb_net_code = r#"
Imports System
Imports System.Collections.Generic
Imports WinForms = System.Windows.Forms

Namespace Enterprise.Billing
    Public Interface IInvoiceService
        Sub GenerateInvoice(ByVal orderId As Integer)
        Function GetStatus(ByVal invoiceId As Long) As String
    End Interface

    Public Structure MoneyAmount
        Public Currency As String
        Public Value As Decimal
    End Structure

    Public Enum PaymentStatus
        Pending = 0
        Processing = 1
        Completed = 2
        Failed = 3
    End Enum

    Public MustInherit Class BaseService
        Protected _logger As Object
    End Class

    ' ==========================================================================
    ' Enterprise Invoice Processor Service
    ' Handles ledger posting and COM ERP integration
    ' ==========================================================================
    Public Class InvoiceProcessor
        Inherits BaseService
        Implements IInvoiceService

        Private _connectionString As String
        Public Event InvoiceProcessed(ByVal invoiceId As Long)

        Public Property TimeoutSeconds As Integer = 30

        Public Sub GenerateInvoice(ByVal orderId As Integer) Implements IInvoiceService.GenerateInvoice
            Dim dbConn As Object
            Set dbConn = CreateObject("ADODB.Connection")
            Call PostLedger(orderId)
        End Sub

        Private Function PostLedger( _
            ByVal orderId As Integer, _
            Optional ByVal isRetry As Boolean = False _
        ) As Boolean
            Return True
        End Function

        Public Function GetStatus(ByVal invoiceId As Long) As String Implements IInvoiceService.GetStatus
            Return "Completed"
        End Function
    End Class

    Public Module MathHelpers
        Public Function Add(a As Integer, b As Integer) As Integer
            Return a + b
        End Function
    End Module
End Namespace
"#;

    parsers::parse_vb("src/billing/InvoiceProcessor.vb", vb_net_code, &mut graph);

    // 1. File & Dialect
    let file_ent = graph.get_entity("src/billing/InvoiceProcessor.vb::file").expect("file entity");
    assert_eq!(file_ent.metadata.get("dialect").map(|s| s.as_str()), Some("vb.net"));

    // 2. Imports
    let ext_generic = graph.get_entity("dotnet::System.Collections.Generic").expect("Imports System.Collections.Generic");
    assert_eq!(ext_generic.entity_type, EntityType::Module);
    assert!(graph.has_edge("src/billing/InvoiceProcessor.vb::file", "dotnet::System.Collections.Generic"));

    // 3. Namespace
    let ns_ent = graph.get_entity("src/billing/InvoiceProcessor.vb::ns::Enterprise.Billing").expect("Namespace");
    assert_eq!(ns_ent.name, "Enterprise.Billing");

    // 4. Interface
    let iface_ent = graph.get_entity("src/billing/InvoiceProcessor.vb::interface::IInvoiceService").expect("Interface");
    assert_eq!(iface_ent.entity_type, EntityType::Interface);

    // 5. Structure & Enum
    let struct_ent = graph.get_entity("src/billing/InvoiceProcessor.vb::struct::MoneyAmount").expect("Structure");
    assert_eq!(struct_ent.entity_type, EntityType::Struct);
    let enum_ent = graph.get_entity("src/billing/InvoiceProcessor.vb::enum::PaymentStatus").expect("Enum");
    assert_eq!(enum_ent.metadata.get("kind").map(|s| s.as_str()), Some("enum"));

    // 6. Class & Inheritance
    let cls_ent = graph.get_entity("src/billing/InvoiceProcessor.vb::class::InvoiceProcessor").expect("Class");
    assert_eq!(cls_ent.entity_type, EntityType::Class);
    assert!(cls_ent.docstring.as_ref().unwrap().contains("Enterprise Invoice Processor Service"));
    assert!(graph.has_edge("src/billing/InvoiceProcessor.vb::class::InvoiceProcessor", "src/billing/InvoiceProcessor.vb::class::BaseService"));
    assert!(graph.has_edge("src/billing/InvoiceProcessor.vb::class::InvoiceProcessor", "src/billing/InvoiceProcessor.vb::interface::IInvoiceService"));

    // 7. Event & Property
    let event_ent = graph.get_entity("src/billing/InvoiceProcessor.vb::InvoiceProcessor::event::InvoiceProcessed").expect("Event");
    assert_eq!(event_ent.metadata.get("is_event").map(|s| s.as_str()), Some("true"));
    let prop_ent = graph.get_entity("src/billing/InvoiceProcessor.vb::InvoiceProcessor::Standard::TimeoutSeconds").expect("Property");
    assert_eq!(prop_ent.entity_type, EntityType::Method);

    // 8. Methods & Functions (including multi-line with ' _')
    let sub_gen = graph.get_entity("src/billing/InvoiceProcessor.vb::InvoiceProcessor::GenerateInvoice").expect("Sub GenerateInvoice");
    assert_eq!(sub_gen.entity_type, EntityType::Method);

    let func_post = graph.get_entity("src/billing/InvoiceProcessor.vb::InvoiceProcessor::PostLedger").expect("Function PostLedger");
    assert_eq!(func_post.entity_type, EntityType::Function);
    assert_eq!(func_post.metadata.get("return_type").map(|s| s.as_str()), Some("Boolean"));

    // 9. COM Integration
    let com_adodb = graph.get_entity("com::ADODB.Connection").expect("COM ADODB.Connection");
    assert_eq!(com_adodb.entity_type, EntityType::Module);
    assert!(graph.has_edge("src/billing/InvoiceProcessor.vb::InvoiceProcessor::GenerateInvoice", "com::ADODB.Connection")
        || graph.has_edge("src/billing/InvoiceProcessor.vb::class::InvoiceProcessor", "com::ADODB.Connection"));

    // 10. Intra-Class Calls
    assert!(graph.has_edge("src/billing/InvoiceProcessor.vb::InvoiceProcessor::GenerateInvoice", "src/billing/InvoiceProcessor.vb::InvoiceProcessor::PostLedger"));

    // 11. Module
    let mod_ent = graph.get_entity("src/billing/InvoiceProcessor.vb::module::MathHelpers").expect("Module MathHelpers");
    assert_eq!(mod_ent.entity_type, EntityType::Module);
}

// =============================================================================
// 2. CLASSIC VB6 & VBA STANDARD MODULE (.bas) PARSER TEST
// =============================================================================
#[test]
fn test_vba_bas_standard_module_parser() {
    let mut graph = CodeGraph::new();
    let vba_bas_code = r#"
Attribute VB_Name = "ModFinancials"
Option Explicit

#If VBA7 Then
    Private Declare PtrSafe Function GetTickCount Lib "kernel32" () As Long
    Public Declare PtrSafe Function SendMessage Lib "user32" Alias "SendMessageA" (ByVal hWnd As LongPtr, ByVal wMsg As Long, ByVal wParam As LongPtr, ByVal lParam As LongPtr) As LongPtr
#Else
    Private Declare Function GetTickCount Lib "kernel32" () As Long
    Public Declare Function SendMessage Lib "user32" Alias "SendMessageA" (ByVal hWnd As Long, ByVal wMsg As Long, ByVal wParam As Long, ByVal lParam As Long) As Long
#End If

Public Type CustomerRecord
    CustomerID As Long
    CustomerName As String * 50
    CreditLimit As Currency
End Type

Public Enum CurrencyType
    USD = 1
    EUR = 2
    GBP = 3
    JPY = 4
End Enum

Public g_TotalProcessed As Long

' Perform batch currency conversion and write to Excel
Public Sub RunBatchConversion(ByVal BatchID As Long)
    Dim xlApp As Object
    Set xlApp = CreateObject("Excel.Application")
    Call LogAudit("Batch started: " & BatchID)
End Sub

Public Function CalculateTax(ByVal Amount As Currency, ByVal Rate As Double) As Currency
    CalculateTax = Amount * Rate
End Function

Private Sub LogAudit(ByVal Message As String)
    ' Internal logging routine
End Sub
"#;

    parsers::parse_vb("vba/modules/ModFinancials.bas", vba_bas_code, &mut graph);

    // 1. Module extraction from Attribute VB_Name
    let mod_ent = graph.get_entity("vba/modules/ModFinancials.bas::ModFinancials").expect("ModFinancials entity");
    assert_eq!(mod_ent.name, "ModFinancials");
    assert_eq!(mod_ent.entity_type, EntityType::Module);

    // 2. Win32 DLL Declarations
    let decl_tick = graph.get_entity("vba/modules/ModFinancials.bas::ModFinancials::GetTickCount").expect("GetTickCount Declare");
    assert_eq!(decl_tick.metadata.get("is_declare").map(|s| s.as_str()), Some("true"));
    assert_eq!(decl_tick.metadata.get("dll").map(|s| s.as_str()), Some("kernel32"));

    let decl_msg = graph.get_entity("vba/modules/ModFinancials.bas::ModFinancials::SendMessage").expect("SendMessage Declare");
    assert_eq!(decl_msg.metadata.get("alias").map(|s| s.as_str()), Some("SendMessageA"));
    assert_eq!(decl_msg.metadata.get("dll").map(|s| s.as_str()), Some("user32"));

    let dll_kernel32 = graph.get_entity("dll::kernel32").expect("dll::kernel32");
    assert!(graph.has_edge(&decl_tick.id, &dll_kernel32.id));

    // 3. User-defined Type & Enum
    let udt_ent = graph.get_entity("vba/modules/ModFinancials.bas::struct::CustomerRecord").expect("CustomerRecord Type");
    assert_eq!(udt_ent.entity_type, EntityType::Struct);

    let enum_ent = graph.get_entity("vba/modules/ModFinancials.bas::enum::CurrencyType").expect("CurrencyType Enum");
    assert_eq!(enum_ent.entity_type, EntityType::Struct);

    // 4. Procedures & Functions
    let sub_batch = graph.get_entity("vba/modules/ModFinancials.bas::ModFinancials::RunBatchConversion").expect("RunBatchConversion");
    assert_eq!(sub_batch.entity_type, EntityType::Method);

    let func_tax = graph.get_entity("vba/modules/ModFinancials.bas::ModFinancials::CalculateTax").expect("CalculateTax");
    assert_eq!(func_tax.entity_type, EntityType::Function);
    assert_eq!(func_tax.metadata.get("return_type").map(|s| s.as_str()), Some("Currency"));

    // 5. COM Excel.Application Reference
    let com_excel = graph.get_entity("com::Excel.Application").expect("com::Excel.Application");
    assert_eq!(com_excel.entity_type, EntityType::Module);
    assert!(graph.has_edge(&sub_batch.id, &com_excel.id)
        || graph.has_edge(&mod_ent.id, &com_excel.id));

    // 6. Call Linkage
    let sub_log = graph.get_entity("vba/modules/ModFinancials.bas::ModFinancials::LogAudit").expect("LogAudit");
    assert!(graph.has_edge(&sub_batch.id, &sub_log.id));
}

// =============================================================================
// 3. VB6 / VBA CLASS MODULE (.cls) & SALESFORCE APEX DISAMBIGUATION TEST
// =============================================================================
#[test]
fn test_vba_cls_and_salesforce_apex_disambiguation() {
    let vb_cls_code = r#"VERSION 1.0 CLASS
BEGIN
  MultiUse = -1  'True
  Persistable = 0  'NotPersistable
  DataBindingBehavior = 0  'vbNone
  DataSourceBehavior  = 0  'vbNone
  MTSTransactionMode  = 0  'NotAnMTSObject
END
Attribute VB_Name = "CInvoiceManager"
Attribute VB_GlobalNameSpace = False
Attribute VB_Creatable = True
Attribute VB_PredeclaredId = False
Attribute VB_Exposed = True
Option Explicit

Implements IProcessable

Private m_InvoiceID As Long
Private m_Status As String

Public Property Get InvoiceID() As Long
    InvoiceID = m_InvoiceID
End Property

Public Property Let InvoiceID(ByVal Value As Long)
    m_InvoiceID = Value
End Property

Public Property Get Status() As String
    Status = m_Status
End Property

Public Property Let Status(ByVal Value As String)
    m_Status = Value
End Property

Public Sub ExecuteProcessing()
    Dim rs As Object
    Set rs = CreateObject("ADODB.Recordset")
End Sub
"#;

    let apex_cls_code = r#"
public with sharing class AccountTriggerHandler {
    public static void handleAfterInsert(List<Account> newAccounts) {
        Set<Id> accIds = new Set<Id>();
        for (Account a : newAccounts) {
            accIds.add(a.Id);
        }
    }
}
"#;

    // Verify disambiguation helper
    assert!(parsers::is_vb_cls(vb_cls_code));
    assert!(!parsers::is_vb_cls(apex_cls_code));

    // Test parsing VB .cls
    let mut vb_graph = CodeGraph::new();
    parsers::parse_generic_source("src/vba/CInvoiceManager.cls", vb_cls_code, "cls", &mut vb_graph);

    let cls_ent = vb_graph.get_entity("src/vba/CInvoiceManager.cls::CInvoiceManager").expect("VB Class CInvoiceManager");
    assert_eq!(cls_ent.name, "CInvoiceManager");
    assert_eq!(cls_ent.entity_type, EntityType::Class);

    // Verify Properties (Get & Let)
    let prop_get_id = vb_graph.get_entity("src/vba/CInvoiceManager.cls::CInvoiceManager::Get::InvoiceID").expect("Property Get InvoiceID");
    assert_eq!(prop_get_id.metadata.get("property_kind").map(|s| s.as_str()), Some("Get"));

    let prop_let_id = vb_graph.get_entity("src/vba/CInvoiceManager.cls::CInvoiceManager::Let::InvoiceID").expect("Property Let InvoiceID");
    assert_eq!(prop_let_id.metadata.get("property_kind").map(|s| s.as_str()), Some("Let"));

    // Verify COM ADODB.Recordset
    let com_rs = vb_graph.get_entity("com::ADODB.Recordset").expect("com::ADODB.Recordset");
    assert_eq!(com_rs.entity_type, EntityType::Module);

    // Test parsing Apex .cls through universal generic parser
    let mut apex_graph = CodeGraph::new();
    parsers::parse_generic_source("force-app/main/default/classes/AccountTriggerHandler.cls", apex_cls_code, "cls", &mut apex_graph);

    let apex_cls = apex_graph.get_entity("force-app/main/default/classes/AccountTriggerHandler.cls::AccountTriggerHandler").expect("Apex Class");
    assert_eq!(apex_cls.name, "AccountTriggerHandler");
    assert_eq!(apex_cls.entity_type, EntityType::Class);
}

// =============================================================================
// 4. VB6 USERFORM (.frm) PARSER TEST
// =============================================================================
#[test]
fn test_vb6_form_parser() {
    let mut graph = CodeGraph::new();
    let frm_code = r#"VERSION 5.00
Begin VB.Form FrmMain 
   Caption         =   "Enterprise Dashboard"
   ClientHeight    =   4800
   ClientWidth     =   6400
   Begin VB.CommandButton btnProcess 
      Caption         =   "Process Batch"
      Height          =   495
      Left            =   1200
      Top             =   2400
      Width           =   1695
   End
   Begin VB.TextBox txtOutput 
      Height          =   1575
      Left            =   480
      Top             =   480
      Width           =   4815
   End
End
Attribute VB_Name = "FrmMain"
Attribute VB_GlobalNameSpace = False
Attribute VB_Creatable = False
Attribute VB_PredeclaredId = True
Attribute VB_Exposed = False
Option Explicit

Private Sub btnProcess_Click()
    Dim fso As Object
    Set fso = CreateObject("Scripting.FileSystemObject")
    txtOutput.Text = "Completed"
    Call RefreshStats
End Sub

Private Sub RefreshStats()
    ' update GUI stats
End Sub
"#;

    parsers::parse_vb("forms/FrmMain.frm", frm_code, &mut graph);

    // 1. Form entity from Attribute VB_Name
    let form_ent = graph.get_entity("forms/FrmMain.frm::FrmMain").expect("FrmMain entity");
    assert_eq!(form_ent.name, "FrmMain");

    // 2. Form GUI Controls extracted
    let btn_ent = graph.get_entity("forms/FrmMain.frm::control::btnProcess").expect("btnProcess control");
    assert_eq!(btn_ent.metadata.get("control_type").map(|s| s.as_str()), Some("VB.CommandButton"));
    assert!(graph.has_edge(&form_ent.id, &btn_ent.id));

    let txt_ent = graph.get_entity("forms/FrmMain.frm::control::txtOutput").expect("txtOutput control");
    assert_eq!(txt_ent.metadata.get("control_type").map(|s| s.as_str()), Some("VB.TextBox"));

    // 3. Event handler & subroutine
    let click_sub = graph.get_entity("forms/FrmMain.frm::FrmMain::btnProcess_Click").expect("btnProcess_Click");
    assert_eq!(click_sub.entity_type, EntityType::Method);

    let refresh_sub = graph.get_entity("forms/FrmMain.frm::FrmMain::RefreshStats").expect("RefreshStats");
    assert_eq!(refresh_sub.entity_type, EntityType::Method);

    // 4. COM Scripting.FileSystemObject reference
    let com_fso = graph.get_entity("com::Scripting.FileSystemObject").expect("com::Scripting.FileSystemObject");
    assert!(graph.has_edge(&click_sub.id, &com_fso.id)
        || graph.has_edge(&form_ent.id, &com_fso.id));

    // 5. Calls edge between click event and RefreshStats
    assert!(graph.has_edge(&click_sub.id, &refresh_sub.id));
}

// =============================================================================
// 5. VBSCRIPT (.vbs) PARSER TEST
// =============================================================================
#[test]
fn test_vbscript_parser() {
    let mut graph = CodeGraph::new();
    let vbs_code = r#"
Dim g_FSO, g_Shell
Set g_FSO = WScript.CreateObject("Scripting.FileSystemObject")
Set g_Shell = CreateObject("WScript.Shell")

Sub Main()
    Dim folderPath
    folderPath = "C:\AuditLogs"
    Call ProcessFolder(folderPath)
End Sub

Sub ProcessFolder(ByVal path)
    Dim folder
    Set folder = g_FSO.GetFolder(path)
End Sub

Class LogHelper
    Private m_LogPath
    Public Sub SetLog(path)
        m_LogPath = path
    End Sub
End Class
"#;

    parsers::parse_vb("scripts/backup.vbs", vbs_code, &mut graph);

    // 1. Dialect metadata
    let file_ent = graph.get_entity("scripts/backup.vbs::file").expect("file entity");
    assert_eq!(file_ent.metadata.get("dialect").map(|s| s.as_str()), Some("vbscript"));

    // 2. Routines
    let sub_main = graph.get_entity("scripts/backup.vbs::backup::Main").expect("Sub Main");
    assert_eq!(sub_main.entity_type, EntityType::Method);

    let sub_proc = graph.get_entity("scripts/backup.vbs::backup::ProcessFolder").expect("Sub ProcessFolder");
    assert_eq!(sub_proc.entity_type, EntityType::Method);

    // 3. Class inside VBScript
    let cls_log = graph.get_entity("scripts/backup.vbs::class::LogHelper").expect("Class LogHelper");
    assert_eq!(cls_log.entity_type, EntityType::Class);

    // 4. WScript / COM references
    let com_fso = graph.get_entity("com::Scripting.FileSystemObject").expect("com::Scripting.FileSystemObject");
    assert_eq!(com_fso.entity_type, EntityType::Module);

    let com_shell = graph.get_entity("com::WScript.Shell").expect("com::WScript.Shell");
    assert_eq!(com_shell.entity_type, EntityType::Module);

    // 5. Inter-procedure call
    assert!(graph.has_edge(&sub_main.id, &sub_proc.id));
}

// =============================================================================
// 6. END-TO-END INDEXER & HIPPOCAMPAL PPR RETRIEVAL TEST
// =============================================================================
#[tokio::test]
async fn test_end_to_end_vb_indexing_and_hippocampal_retrieval() {
    let temp_dir = std::env::temp_dir().join(format!("rumin_vb_test_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&temp_dir).unwrap();

    // 1. Create VB.NET service file
    let vb_file = temp_dir.join("PaymentProcessor.vb");
    fs::write(
        &vb_file,
        r#"
Namespace FinancialServices
    Public Class PaymentProcessor
        Public Sub AuthorizePayment(ByVal accountNo As String, ByVal amount As Decimal)
            Dim adodb As Object
            Set adodb = CreateObject("ADODB.Connection")
            Call ValidateFraud(accountNo)
        End Sub

        Private Function ValidateFraud(ByVal accountNo As String) As Boolean
            Return True
        End Function
    End Class
End Namespace
"#,
    ).unwrap();

    // 2. Create VBA Module file
    let bas_file = temp_dir.join("LedgerExport.bas");
    fs::write(
        &bas_file,
        r#"
Attribute VB_Name = "LedgerExport"
Public Sub ExportToExcel()
    Dim xlApp As Object
    Set xlApp = CreateObject("Excel.Application")
End Sub
"#,
    ).unwrap();

    // 3. Create VB .cls Class file
    let vb_cls_file = temp_dir.join("TaxCalculator.cls");
    fs::write(
        &vb_cls_file,
        r#"VERSION 1.0 CLASS
BEGIN
  MultiUse = -1  'True
END
Attribute VB_Name = "TaxCalculator"
Option Explicit

Public Function ComputeVAT(ByVal GrossAmount As Double) As Double
    ComputeVAT = GrossAmount * 0.20
End Function
"#,
    ).unwrap();

    // 4. Create Salesforce Apex .cls side-by-side in same repo to test co-existence
    let apex_cls_file = temp_dir.join("CaseTriggerHandler.cls");
    fs::write(
        &apex_cls_file,
        r#"
public with sharing class CaseTriggerHandler {
    public static void run() {}
}
"#,
    ).unwrap();

    let indexer = Indexer::new(&temp_dir);
    let mut graph = CodeGraph::new();
    let sidecar = SidecarClient::new("dummy");

    let stats = indexer.sync_repository(&mut graph, &sidecar, false).await.expect("sync_repository");
    assert!(stats.total_indexed >= 4, "Must index all 4 files");

    // Verify VB entities in graph
    let pay_proc = graph.get_entity("PaymentProcessor.vb::class::PaymentProcessor").expect("PaymentProcessor");
    assert_eq!(pay_proc.entity_type, EntityType::Class);

    let export_xl = graph.get_entity("LedgerExport.bas::LedgerExport::ExportToExcel").expect("ExportToExcel");
    assert_eq!(export_xl.entity_type, EntityType::Method);

    let compute_vat = graph.get_entity("TaxCalculator.cls::TaxCalculator::ComputeVAT").expect("ComputeVAT in VB .cls");
    assert_eq!(compute_vat.entity_type, EntityType::Function);

    let apex_case = graph.get_entity("CaseTriggerHandler.cls::CaseTriggerHandler").expect("CaseTriggerHandler in Apex .cls");
    assert_eq!(apex_case.entity_type, EntityType::Class);

    // Verify COM references
    assert!(graph.get_entity("com::ADODB.Connection").is_some());
    assert!(graph.get_entity("com::Excel.Application").is_some());

    // Test Hippocampal Personalized PageRank Associative Search
    let engine = HippocampalEngine::default();
    let budget = ContextBudget {
        total_token_limit: 1500,
        max_symbols: 5,
        include_signatures: true,
        expand_bodies: false,
        types: None,
        path_prefix: None,
        dampen_hubs: false,
    };

    let query_res = engine.query(&graph, "ADODB.Connection PaymentProcessor fraud", &budget);
    assert!(!query_res.top_symbols.is_empty(), "QueryResult top_symbols must not be empty");
    let names: Vec<String> = query_res.top_symbols.iter().map(|e| e.name.clone()).collect();
    assert!(names.iter().any(|n| n.contains("PaymentProcessor") || n.contains("ADODB.Connection")), "Must retrieve relevant VB entity: {:?}", names);

    // Cleanup
    let _ = fs::remove_dir_all(&temp_dir);
}
