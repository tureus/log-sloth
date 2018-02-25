use serde_xml_rs::deserialize;

#[derive(Debug, Deserialize, Default)]
pub struct Event {
    //xmlns: String, // this property cannot be extracted. not sure why. but OK!
    System: System,
    UserData: Option<UserData>,
}

#[derive(Debug, Deserialize, Default)]
pub struct System{
    Provider: Provider,
    EventID: usize,
    Version: usize,
    Level: usize,
    Task: usize,
    Opcode: usize,
    Keywords: String,
    TimeCreated: TimeCreated,
    EventRecordID: usize,
    Correlation: Option<String>, // not sure how to test empty stuff is there...
    Execution: Execution,
    Channel: String,
    Computer: String,
    Security: Security,
}

#[derive(Debug, Deserialize, Default)]
pub struct Provider {
    Name: String,
    Guid: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct TimeCreated {
    SystemTime: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct Execution {
    ProcessID: String,
    ThreadID: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct Security {
    UserID: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct UserData {
    RuleAndFileData: RuleAndFileData,
}

#[derive(Debug, Deserialize, Default)]
pub struct RuleAndFileData {
    PolicyName: String,
    RuleId: String,
    RuleName: String,
    RuleSddl: String,
    TargetUser: String,
    TargetProcessId: String,
    FilePath: String,
    FileHash: String,
    Fqbn: String,
}


#[test]
fn test_parse() {
    let xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<Event xmlns="http://schemas.microsoft.com/win/2004/08/events/event">
  <System>
    <Provider Name="Microsoft-Windows-AppLocker" Guid="{CBDA4DBF-8D5D-4F69-9578-BE14AA540D22}" />
    <EventID>8003</EventID>
    <Version>0</Version>
    <Level>3</Level>
    <Task>0</Task>
    <Opcode>0</Opcode>
    <Keywords>0x8000000000000000</Keywords>
    <TimeCreated SystemTime="2017-12-15T22:01:14.820440800Z" />
    <EventRecordID>25033</EventRecordID>
    <Correlation />
    <Execution ProcessID="1120" ThreadID="1336" />
    <Channel>Microsoft-Windows-AppLocker/EXE and DLL</Channel>
    <Computer>LP-mr-computer.hq.corp.ecorp.com</Computer>
    <Security UserID="S-1-5-11-1111111111-1407861736-3805098287-61855" />
  </System>
  <UserData>
    <RuleAndFileData xmlns="http://schemas.microsoft.com/schemas/event/Microsoft.Windows/1.0.0.0" xmlns:auto-ns2="http://schemas.microsoft.com/win/2004/08/events">
      <PolicyName>EXE</PolicyName>
      <RuleId>{00000000-0000-0000-0000-000000000000}</RuleId>
      <RuleName>-</RuleName>
      <RuleSddl>-</RuleSddl>
      <TargetUser>S-1-5-21-1475062817-1407861736-3805098287-61855</TargetUser>
      <TargetProcessId>8780</TargetProcessId>
      <FilePath>%OSDRIVE%\USERS\JDOE\APPDATA\LOCAL\ATLASSIAN\HIPCHAT4\QTWEBENGINEPROCESS.EXE</FilePath>
      <FileHash>E5DA617777BCF01F0F9A51796FF42ED2C6F8190515E168A38D05706BC81F9BCD</FileHash>
      <Fqbn>-</Fqbn>
    </RuleAndFileData>
  </UserData>
</Event>"##;

    let event : Event = deserialize(xml.as_bytes()).unwrap();

    assert_eq!(format!("{:?}",event), "1.0");
}