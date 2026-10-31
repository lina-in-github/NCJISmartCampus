增加api,并把当前用户登录的方式改为不对称加密：前端通过密钥派生函数生成公钥与私钥并把公钥上交服务端，此后服务端通过挑战加密验证。
1、logintry:
{username:...}
返回：
{loginSessionID:str,challengeEncrypt:byte[],answerEncryptingKey:byte[]}
2、getAuths：
{loginSessionID:str,encryptedAnswer:byte[]}
返回：
{
    auths:Array<str>//all auth of this user
}
3、resetPassword

{loginSessionID:str,encryptedAnswer:byte[],newPasswordPubKey:byte[]}
返回：success/fail
